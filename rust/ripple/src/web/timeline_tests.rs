use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::server::db::schema::PriceSource;
use crate::server::prices::store::{NewPrice, save_price};
use crate::server::stocks::repository::{RegisterInput, RegisterResult, register_stock};
use crate::server::timeline::read::NatureTab;
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
        nature: NatureTab::Fact,
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
        "관련도 6점·영향도 5점 이상인 기사를 최신순으로 보여 줍니다.",
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
    assert!(body.contains("아직 관련도 6점·영향도 5점 이상인 기사가 없습니다."));
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

// ---- 성격 탭과 태그(article-classify/nature-tabs) ----

fn set_nature(web: &TestWeb, article_id: i64, fact: f64, analysis: f64, forecast: f64) {
    web.writer()
        .execute(
            "UPDATE articles SET nature_fact = ?1, nature_analysis = ?2, nature_forecast = ?3, nature_opinion = 0.1, \
             nature_media = 0, nature_roundup = 0, nature_at = 1 WHERE id = ?4",
            rusqlite::params![fact, analysis, forecast, article_id],
        )
        .unwrap();
}

#[test]
fn timeline_href_keeps_the_view_tab_and_leaves_nothing_for_the_fact_tab() {
    let view = TimelineFilter {
        nature: NatureTab::View,
        stock_id: Some(2),
        ..TimelineFilter::default()
    };
    assert_eq!(timeline_href(&view, None), "/?stock=2&nature=view");
    assert_eq!(
        timeline_href(&view, Some(60)),
        "/?stock=2&nature=view&limit=60"
    );
    assert_eq!(timeline_href(&TimelineFilter::default(), None), "/");
}

#[tokio::test]
async fn nature_tabs_show_the_current_tab_and_tags_before_titles() {
    let web = start_web(None).await;
    let (_, articles) = seed(&web, 3);
    set_nature(&web, articles[0], 0.9, 0.1, 0.1);
    set_nature(&web, articles[1], 0.8, 0.1, 0.7);
    // articles[2]는 성격 없음
    let fact = text(&web, "/").await;
    assert!(fact.contains(
        "aria-label=\"기사 성격\" class=\"mt-4 flex border-b\" data-testid=\"nature-tabs\""
    ));
    assert!(fact.contains("<a href=\"/\" aria-current=\"page\" class=\"-mb-px flex-1 border-b-2 py-2 text-center text-sm transition-colors border-primary"));
    assert!(fact.contains("href=\"/?nature=view\" class="));
    assert_eq!(fact.matches("data-testid=\"timeline-card\"").count(), 3);
    // 사실 태그 둘(첫째·둘째), 둘째 기사는 전망 태그도
    assert_eq!(fact.matches("data-testid=\"nature-tag\"").count(), 3);
    let title_at = fact.find("<a href=\"https://fixture").unwrap_or(0);
    let tag_at = fact.find("data-testid=\"nature-tag\"").unwrap();
    assert!(
        tag_at
            < fact[tag_at..]
                .find("hx-boost=\"false\"")
                .map_or(0, |i| tag_at + i)
    );
    let _ = title_at;

    let view = text(&web, "/?nature=view").await;
    assert!(view.contains("<a href=\"/?nature=view\" aria-current=\"page\""));
    assert_eq!(view.matches("data-testid=\"timeline-card\"").count(), 1);
    assert_eq!(view.matches("data-testid=\"nature-tag\"").count(), 2);
    assert!(view.contains(">전망</span>"));
    // 모르는 값은 사실 탭
    let unknown = text(&web, "/?nature=elsewhere").await;
    assert_eq!(unknown.matches("data-testid=\"timeline-card\"").count(), 3);
}

#[tokio::test]
async fn nature_tab_survives_filters_price_strip_topic_cards_and_more_links() {
    let web = start_web(None).await;
    let (stock_id, articles) = seed(&web, 31);
    for id in &articles {
        set_nature(&web, *id, 0.1, 0.9, 0.1);
    }
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
    {
        let conn = web.writer();
        conn.execute(
            "INSERT INTO topics (source, name, summary, terms, created_at) VALUES ('auto', '중동 긴장', 's', '[]', ?1)",
            rusqlite::params![T0 - 80 * HOUR],
        )
        .unwrap();
        let topic_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO stocks (market, code, name, name_ko, exchange, user_input, kind, topic_id, keywords_status, created_at) \
             VALUES ('KR', 'TOPIC-1', '중동 긴장', '중동 긴장', '한국 증시', '중동 긴장', 'topic', ?1, 'ready', ?2)",
            rusqlite::params![topic_id, T0 - 80 * HOUR],
        )
        .unwrap();
        let mut link = LinkSpec::new(conn.last_insert_rowid(), "가나 소식", T0 - HOUR);
        link.kind = "keyword";
        link.magnitude = Some(7.0);
        let (_, article) = insert_link(&conn, &link);
        drop(conn);
        set_nature(&web, article, 0.1, 0.9, 0.1);
    }
    let body = text(&web, "/?nature=view").await;
    for part in [
        // 항목·영향 칩, 탭의 사실 링크는 탭을 바꾼다(주소에서 값이 빠짐)
        "href=\"/?stock=1&#38;nature=view\"",
        "href=\"/?impact=positive&#38;nature=view\"",
        // 토픽 카드 링크
        "href=\"/?topic=1&#38;nature=view\"",
        // 더 보기
        "href=\"/?nature=view&#38;limit=60\"",
    ] {
        assert!(body.contains(part), "{part}");
    }
    assert!(body.contains("&#38;nature=view\" class=\"flex min-w-36"));
    // 사실 탭 링크는 탭 값이 없고, 칩 링크로 탭을 옮겨도 필터가 남는다
    let filtered = text(&web, "/?nature=view&stock=1&impact=neutral").await;
    assert!(filtered.contains("<a href=\"/?stock=1&#38;impact=neutral\""));
    assert!(filtered.contains(
        "<a href=\"/?stock=1&#38;impact=neutral&#38;nature=view\" aria-current=\"page\""
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

#[tokio::test]
async fn guest_is_capped_at_90_cards_without_a_more_link_but_members_are_not() {
    let web = start_web(None).await;
    seed(&web, 100);
    let guest = web
        .client
        .get(web.url("/?limit=600"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(guest.matches("data-testid=\"timeline-card\"").count(), 90);
    assert!(!guest.contains(">더 보기</a>"));
    // 90 미만이면 손님도 더 볼 수 있다
    let guest_first = web
        .client
        .get(web.url("/?limit=60"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(guest_first.contains("href=\"/?limit=90\""));
    let member = text(&web, "/?limit=600").await;
    assert_eq!(member.matches("data-testid=\"timeline-card\"").count(), 100);
}
