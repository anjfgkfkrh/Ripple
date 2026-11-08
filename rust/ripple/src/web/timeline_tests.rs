use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::server::db::schema::PriceSource;
use crate::server::prices::store::{NewPrice, save_price};
use crate::server::stocks::repository::{RegisterInput, RegisterResult, register_stock};
use crate::server::timeline::reads::set_articles_read;
use crate::test_support::{LinkSpec, insert_link, seed_listings};
use crate::web::test_support::{T0, TestWeb, start_web};

const HOUR: i64 = 60 * 60 * 1000;

// ---- 숫자 모양(Rust에서만 생기는 경우: Intl.NumberFormat·toFixed를 손으로 옮김) ----

#[test]
fn format_number_matches_intl_for_korean_prices() {
    assert_eq!(format_number(83_500.0, 0, 0), "83,500");
    assert_eq!(format_number(1_234_567.89, 0, 0), "1,234,568");
    assert_eq!(format_number(999.5, 0, 0), "1,000");
    assert_eq!(format_number(999.49, 0, 0), "999");
    assert_eq!(format_number(0.0, 0, 0), "0");
    assert_eq!(format_number(0.4, 0, 0), "0");
    assert_eq!(format_number(100.0, 0, 0), "100");
    assert_eq!(format_number(1_000.0, 0, 0), "1,000");
}

#[test]
fn format_number_matches_intl_for_us_prices() {
    assert_eq!(format_number(1_234.5, 2, 2), "1,234.50");
    assert_eq!(format_number(0.0, 2, 2), "0.00");
    assert_eq!(format_number(12.0, 2, 2), "12.00");
    // Intl은 짧게 되읽히는 십진수에서 반올림해 toFixed(1.00)와 다르다
    assert_eq!(format_number(1.005, 2, 2), "1.01");
    assert_eq!(format_number(99.995, 2, 2), "100.00");
    assert_eq!(format_number(0.999, 2, 2), "1.00");
    assert_eq!(format_number(123.456, 2, 2), "123.46");
    assert_eq!(format_number(1_000_000.126, 2, 2), "1,000,000.13");
}

#[test]
fn format_number_signs_and_odd_values() {
    assert_eq!(format_number(-1_500.0, 0, 0), "-1,500");
    assert_eq!(format_number(-0.2, 0, 0), "0");
    assert_eq!(format_number(-0.004, 2, 2), "0.00");
    assert_eq!(format_number(f64::NAN, 0, 0), "NaN");
}

#[test]
fn timeline_href_keeps_filters_and_resets_limit() {
    let none = TimelineFilter::default();
    assert_eq!(timeline_href(&none, None), "/");
    assert_eq!(timeline_href(&none, Some(30)), "/");
    assert_eq!(timeline_href(&none, Some(60)), "/?limit=60");
    let all = TimelineFilter {
        stock_id: Some(2),
        topic_id: Some(7),
        impact: Some(Impact::Negative),
    };
    assert_eq!(
        timeline_href(&all, Some(90)),
        "/?stock=2&topic=7&impact=negative&limit=90"
    );
}

// ---- 화면 ----

fn seed(web: &TestWeb, count: usize) -> (i64, Vec<i64>) {
    let mut conn = web.writer();
    seed_listings(&mut conn);
    let RegisterResult::Created(stock) = register_stock(
        &conn,
        &RegisterInput {
            market: Market::Kr,
            code: "005930",
            user_input: "삼성전자",
            name_ko: None,
        },
        T0,
    )
    .unwrap() else {
        panic!("표본에 없음");
    };
    let articles = (0..count)
        .map(|i| {
            let title: String = format!(
                "{}{} 소식",
                char::from_u32(0xac00 + i as u32 * 97).unwrap(),
                char::from_u32(0xac00 + i as u32 * 97 + 41).unwrap()
            );
            let mut link = LinkSpec::new(stock.id, &title, T0 - (i as i64 + 1) * HOUR);
            link.magnitude = Some(7.0);
            insert_link(&conn, &link).1
        })
        .collect();
    (stock.id, articles)
}

async fn text(web: &TestWeb, path: &str) -> String {
    web.get(path).send().await.unwrap().text().await.unwrap()
}

#[tokio::test]
async fn empty_database_invites_registering_stocks() {
    let web = start_web(None).await;
    let body = text(&web, "/").await;
    assert!(body.contains("<h1 class=\"text-xl font-semibold\">타임라인</h1>"));
    assert!(body.contains("등록한 종목이 없습니다."));
    assert!(body.contains("href=\"/stocks\""));
    assert!(!body.contains("data-testid=\"timeline-filters\""));
}

#[tokio::test]
async fn shows_cards_by_day_with_filters_and_threshold_text() {
    let web = start_web(None).await;
    seed(&web, 2);
    let body = text(&web, "/").await;
    for part in [
        "관련도 6점·영향도 6점 이상인 기사를 최신순으로 보여 줍니다.",
        "data-testid=\"timeline-filters\"",
        "aria-label=\"항목 필터\"",
        "aria-label=\"영향 필터\"",
        "data-testid=\"timeline-day\"",
        "오늘 · 10월 19일(월)",
        "data-testid=\"timeline-card\"",
        "data-read=\"false\"",
        "삼성전자",
        "관련도 7.0 · 영향도 7.0",
        "기사에 종목 이름이 나옴",
        "읽음으로 표시",
        "noindex, nofollow",
    ] {
        assert!(body.contains(part), "{part}");
    }
    // 평가가 없는 기사는 보이지 않고, 칩은 이 종목 하나
    assert!(!body.contains("더 보기"));
    assert!(body.contains("href=\"/?stock=1\""));
}

#[tokio::test]
async fn htmx_gets_only_the_fragment_but_history_restore_gets_the_page() {
    let web = start_web(None).await;
    seed(&web, 1);
    let fragment = web
        .get("/")
        .header("hx-request", "true")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        fragment.starts_with("\n<div id=\"timeline-body\"")
            || fragment.contains("id=\"timeline-body\"")
    );
    assert!(!fragment.contains("<html"));
    assert!(!fragment.contains("<header"));
    let restore = web
        .get("/")
        .header("hx-request", "true")
        .header("hx-history-restore-request", "true")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(restore.contains("<html"));
    assert!(restore.contains("id=\"timeline-body\""));
}

#[tokio::test]
async fn chips_boost_in_place_and_stocks_link_does_not() {
    let web = start_web(None).await;
    seed(&web, 1);
    let body = text(&web, "/").await;
    assert!(body.contains(
        "id=\"timeline-body\" hx-boost=\"true\" hx-target=\"#timeline-body\" hx-swap=\"outerHTML\" hx-push-url=\"true\""
    ));
    // 원문 링크는 새 탭이라 boost하지 않는다
    assert!(body.contains(
        "target=\"_blank\" rel=\"noopener noreferrer\" class=\"hover:underline\" hx-boost=\"false\""
    ));
    assert!(body.contains("scrollIntoViewOnBoost\":false"));
}

#[tokio::test]
async fn more_button_and_limit_in_the_address() {
    let web = start_web(None).await;
    seed(&web, 31);
    let first = text(&web, "/").await;
    assert_eq!(first.matches("data-testid=\"timeline-card\"").count(), 30);
    assert!(first.contains("href=\"/?limit=60\""));
    assert!(first.contains(">더 보기</a>"));
    let more = text(&web, "/?limit=60").await;
    assert_eq!(more.matches("data-testid=\"timeline-card\"").count(), 31);
    assert!(!more.contains(">더 보기</a>"));
}

#[tokio::test]
async fn filter_with_no_match_says_so_and_unknown_values_are_ignored() {
    let web = start_web(None).await;
    seed(&web, 1);
    let none = text(&web, "/?impact=positive").await;
    assert!(none.contains("data-testid=\"timeline-empty\""));
    assert!(none.contains("조건에 맞는 기사가 없습니다."));
    let unknown = text(&web, "/?stock=99&impact=great").await;
    assert!(unknown.contains("data-testid=\"timeline-card\""));
}

#[tokio::test]
async fn nothing_over_the_bar_says_so() {
    let web = start_web(None).await;
    {
        let mut conn = web.writer();
        seed_listings(&mut conn);
        register_stock(
            &conn,
            &RegisterInput {
                market: Market::Kr,
                code: "005930",
                user_input: "삼성전자",
                name_ko: None,
            },
            T0,
        )
        .unwrap();
    }
    let body = text(&web, "/").await;
    assert!(body.contains("아직 관련도 6점·영향도 6점 이상인 기사가 없습니다."));
}

#[tokio::test]
async fn read_cards_are_dimmed_and_labelled() {
    let web = start_web(None).await;
    let (_, articles) = seed(&web, 2);
    set_articles_read(&web.writer(), &[articles[0]], true, T0).unwrap();
    let body = text(&web, "/").await;
    assert_eq!(body.matches("data-read=\"true\"").count(), 1);
    assert_eq!(body.matches("data-testid=\"read-label\"").count(), 1);
    assert!(body.contains("안 읽음으로"));
    assert!(body.contains("group rounded-lg border p-4 bg-muted/40"));
    assert!(body.contains(&format!("data-article-ids=\"{}\"", articles[0])));
}

#[tokio::test]
async fn pending_evaluations_are_announced() {
    let web = start_web(None).await;
    let (stock_id, _) = seed(&web, 1);
    let mut link = LinkSpec::new(stock_id, "평가 전", T0 - HOUR);
    link.relevance = None;
    insert_link(&web.writer(), &link);
    let body = text(&web, "/").await;
    assert!(body.contains("평가를 기다리는 기사 1건은 평가가 끝나면 기준에 맞는 것만 보입니다."));
}

#[tokio::test]
async fn price_strip_shows_direction_and_toggles_the_stock_filter() {
    let web = start_web(None).await;
    let (stock_id, _) = seed(&web, 1);
    save_price(
        &web.writer(),
        &NewPrice {
            stock_id,
            price: 83_500.0,
            change: 1_200.0,
            change_percent: 1.4578,
            source: PriceSource::Kis,
            quoted_at: Some(T0 - 60_000),
            fetched_at: T0,
        },
    )
    .unwrap();
    let body = text(&web, "/").await;
    assert!(body.contains("data-testid=\"price-strip\""));
    assert!(body.contains("83,500<span class=\"ml-0.5 text-xs font-normal\">원</span>"));
    assert!(body.contains("▲1,200 +1.46%"));
    assert!(body.contains("text-red-600 dark:text-red-400"));
    assert!(body.contains("href=\"/?stock=1\""));
    // 고른 종목을 한 번 더 누르면 풀린다
    let selected = text(&web, "/?stock=1").await;
    assert!(selected.contains("aria-current=\"true\" class=\"flex min-w-36"));
    assert!(selected.contains("<a href=\"/\" aria-current=\"true\" class=\"flex min-w-36"));
}

#[tokio::test]
async fn topic_cards_show_active_and_faded_issues() {
    let web = start_web(None).await;
    let (stock_id, _) = seed(&web, 1);
    let conn = web.writer();
    let mut topics = Vec::new();
    for name in ["중동 긴장", "미중 갈등"] {
        conn.execute(
            "INSERT INTO topics (source, name, summary, terms, created_at) VALUES ('auto', ?1, ?2, '[]', ?3)",
            rusqlite::params![name, format!("{name} 설명"), T0 - 80 * HOUR],
        )
        .unwrap();
        let topic_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO stocks (market, code, name, name_ko, exchange, user_input, kind, topic_id, keywords_status, created_at) \
             VALUES ('KR', ?1, ?2, ?2, '한국 증시', ?2, 'topic', ?3, 'ready', ?4)",
            rusqlite::params![format!("TOPIC-{topic_id}"), name, topic_id, T0 - 80 * HOUR],
        )
        .unwrap();
        topics.push((topic_id, conn.last_insert_rowid()));
    }
    for (index, hours) in [(0usize, 1i64), (1, 13)] {
        let mut link = LinkSpec::new(
            topics[index].1,
            if index == 0 {
                "가나 소식"
            } else {
                "다라 소식"
            },
            T0 - hours * HOUR,
        );
        link.kind = "keyword";
        link.magnitude = Some(7.0);
        insert_link(&conn, &link);
    }
    let _ = stock_id;
    let body = text(&web, "/").await;
    for part in [
        "data-testid=\"topic-cards\"",
        "지금 시장 이슈",
        "중동 긴장",
        "중동 긴장 설명",
        "기사 1건 · 마지막 오늘 08:00",
        "이전 이슈 1개",
        "<details class=\"group mt-1\">",
        "opacity-75",
    ] {
        assert!(body.contains(part), "{part}");
    }
    // 접힌 토픽을 고르면 펼쳐 두고 안내 문구를 보인다
    let faded_id = topics[1].0;
    let picked = text(&web, &format!("/?topic={faded_id}")).await;
    assert!(picked.contains("<details class=\"group mt-1\" open>"));
    assert!(picked.contains("data-testid=\"topic-filter\""));
    assert!(picked.contains("<strong>미중 갈등</strong> 기사만 보는 중"));
    assert!(picked.contains(
        "href=\"/\" class=\"text-muted-foreground underline underline-offset-2\">모두 보기"
    ));
}

// ---- 읽음 저장 ----

#[test]
fn read_request_is_checked_again() {
    assert_eq!(
        parse_read_request(br#"{"articleIds":[1,2],"read":true}"#),
        Some((vec![1, 2], true))
    );
    for bad in [
        &br#"{"articleIds":[],"read":true}"#[..],
        br#"{"articleIds":[1],"read":"yes"}"#,
        br#"{"articleIds":[0],"read":true}"#,
        br#"{"articleIds":[1.5],"read":true}"#,
        br#"{"articleIds":"1","read":true}"#,
        br#"{"read":true}"#,
        b"not json",
    ] {
        assert_eq!(
            parse_read_request(bad),
            None,
            "{}",
            String::from_utf8_lossy(bad)
        );
    }
    let many = format!(
        "{{\"articleIds\":[{}],\"read\":true}}",
        (1..=MAX_READ_BATCH + 1)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    assert_eq!(parse_read_request(many.as_bytes()), None);
}

async fn post_read(web: &TestWeb, body: &str) -> Value {
    web.post("/articles/read")
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn set_read_passes_to_the_worker_and_reports_errors() {
    let worker = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/articles/read"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"changed": 2})))
        .up_to_n_times(1)
        .mount(&worker)
        .await;
    Mock::given(method("PUT"))
        .and(path("/articles/read"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({"error": "internal"})))
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    assert_eq!(
        post_read(&web, r#"{"articleIds":[1,2],"read":true}"#).await,
        json!({"ok": true})
    );
    let failed = post_read(&web, r#"{"articleIds":[1,2],"read":true}"#).await;
    assert!(failed["error"].as_str().is_some_and(|e| !e.is_empty()));
    assert_eq!(failed.get("ok"), None);
    let bad = post_read(&web, r#"{"articleIds":[],"read":true}"#).await;
    assert_eq!(bad, json!({"error": "읽음 표시를 저장하지 못했습니다."}));
    let seen = worker.received_requests().await.unwrap();
    let first: Value = serde_json::from_slice(&seen[0].body).unwrap();
    assert_eq!(first, json!({"articleIds": [1, 2], "read": true}));
    // 인증은 worker 토큰으로
    assert!(seen[0].headers.get("authorization").is_some());
}

#[tokio::test]
async fn set_read_without_worker_says_it_is_unreachable() {
    let web = start_web(None).await;
    let result = post_read(&web, r#"{"articleIds":[1],"read":false}"#).await;
    assert!(
        result["error"]
            .as_str()
            .unwrap()
            .contains("worker에 연결할 수 없습니다.")
    );
}

#[tokio::test]
async fn set_read_and_timeline_need_a_login() {
    let web = crate::web::test_support::start_web_guest_off().await;
    let timeline = web.client.get(web.url("/")).send().await.unwrap();
    assert_eq!(timeline.status(), StatusCode::TEMPORARY_REDIRECT);
    let read = web
        .client
        .post(web.url("/articles/read"))
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(read.status(), StatusCode::UNAUTHORIZED);
    // 다른 출처에서 온 쓰기는 막는다
    let cross = web
        .post("/articles/read")
        .header("origin", "https://evil.example")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(cross.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn guest_never_sees_read_state() {
    let web = start_web(None).await;
    let (_, articles) = seed(&web, 2);
    set_articles_read(&web.writer(), &[articles[0]], true, T0).unwrap();
    let guest = web
        .client
        .get(web.url("/"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(guest.matches("data-read=\"true\"").count(), 0);
    assert_eq!(guest.matches("data-read=\"false\"").count(), 2);
    assert!(!guest.contains("data-testid=\"read-label\""));
}
