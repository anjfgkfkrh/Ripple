use rusqlite::{Connection, params};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::server::db::schema::KeywordKind;
use crate::server::keywords::read::Keyword;
use crate::web::test_support::{T0, TestWeb, start_web};

fn keyword(
    id: i64,
    parent_id: Option<i64>,
    text: &str,
    topic_id: Option<i64>,
    kind: KeywordKind,
) -> Keyword {
    Keyword {
        id,
        stock_id: 1,
        level: if parent_id.is_some() { 2 } else { 1 },
        parent_id,
        text_ko: text.to_string(),
        text_en: None,
        reason: String::new(),
        path: text.to_string(),
        created_at: T0,
        kind,
        relevance: None,
        magnitude: None,
        topic_id,
    }
}

/// 트리 조각을 글자로 줄인다: 이름 = 노드, `(` `)` = 들여쓰기, `;` = 노드 끝
fn shape(events: &[TreeEvent]) -> String {
    events
        .iter()
        .map(|event| match event {
            TreeEvent::Node(node) => node.text_ko.clone(),
            TreeEvent::BranchStart => "(".to_string(),
            TreeEvent::BranchEnd => ")".to_string(),
            TreeEvent::NodeEnd => ";".to_string(),
        })
        .collect()
}

#[test]
fn tree_nests_children_under_their_parent_in_list_order() {
    let keywords = vec![
        keyword(1, None, "a", None, KeywordKind::Structure),
        keyword(2, Some(1), "b", None, KeywordKind::Structure),
        keyword(3, Some(2), "c", None, KeywordKind::Structure),
        keyword(4, None, "d", None, KeywordKind::Structure),
    ];
    assert_eq!(shape(&keyword_tree(&keywords)), "a(b(c;););d;");
}

#[test]
fn orphan_keyword_goes_to_the_top_like_the_ts_tree() {
    // 부모가 목록에 없으면 맨 위에 둔다(TS `toTree`의 `parent ?? roots`)
    let keywords = vec![keyword(5, Some(99), "e", None, KeywordKind::Structure)];
    assert_eq!(shape(&keyword_tree(&keywords)), "e;");
}

#[test]
fn top_badges_follow_topic_then_current() {
    let node = |keyword: &Keyword| tree_node(keyword);
    let topic = node(&keyword(1, None, "t", Some(7), KeywordKind::Structure));
    assert!(topic.topic && !topic.current);
    let current = node(&keyword(2, None, "c", None, KeywordKind::Current));
    assert!(current.current && !current.topic);
    // 토픽 가지의 안쪽 키워드에는 배지가 없다
    let inner = node(&keyword(3, Some(1), "i", None, KeywordKind::Current));
    assert!(!inner.current && !inner.topic);
}

#[test]
fn id_must_be_a_positive_integer() {
    assert_eq!(parse_id("12"), Some(12));
    assert_eq!(parse_id("0"), None);
    assert_eq!(parse_id("-3"), None);
    assert_eq!(parse_id("1.5"), None);
    assert_eq!(parse_id(""), None);
}

fn insert_stock(conn: &Connection, code: &str, name: &str, status: &str) -> i64 {
    conn.execute(
        "INSERT INTO stocks (market, code, name, name_ko, exchange, user_input, created_at, kind, keywords_status) \
         VALUES ('KR', ?1, ?2, ?2, 'KOSPI', ?2, ?3, 'stock', ?4)",
        params![code, name, T0, status],
    )
    .unwrap();
    conn.last_insert_rowid()
}

async fn post_form(web: &TestWeb, path: &str, fields: &[(&str, &str)]) -> (u16, String) {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields)
        .finish();
    let response = web
        .post(path)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    (status, response.text().await.unwrap())
}

async fn text(web: &TestWeb, path: &str) -> String {
    web.get(path).send().await.unwrap().text().await.unwrap()
}

#[tokio::test]
async fn stocks_page_needs_a_login() {
    // 손님 보기를 끄면 로그인이 필요하다
    let web = crate::web::test_support::start_web_guest_off().await;
    let response = web.client.get(web.url("/stocks")).send().await.unwrap();
    assert_eq!(response.status().as_u16(), 307);
    assert_eq!(response.headers()["location"], "/login?next=%2Fstocks");
}

#[tokio::test]
async fn empty_stocks_page_shows_sections_and_closed_markets() {
    // 시장 항목을 넣지 않은 DB: 두 증시 모두 꺼짐으로 보인다
    let web = start_web(None).await;
    let body = text(&web, "/stocks").await;
    for part in [
        "<h1 class=\"text-xl font-semibold\">종목</h1>",
        "아직 등록한 종목이 없습니다.",
        "data-testid=\"market-topics\"",
        "한국 증시",
        "미국 증시",
        "꺼짐",
        "아직 등록한 종목이 없습니다.",
        "주제 추가",
        "data-testid=\"market-list\"",
    ] {
        assert!(body.contains(part), "{part}");
    }
    // 만드는 중인 것이 없으면 목록은 3초마다 읽지 않는다
    assert!(!body.contains("hx-get=\"/stocks/list\""));
}

#[tokio::test]
async fn making_keywords_poll_only_their_fragment() {
    let web = start_web(None).await;
    {
        let conn = web.writer();
        insert_stock(&conn, "005930", "삼성전자", "pending");
    }
    let body = text(&web, "/stocks/list").await;
    assert!(body.contains("hx-get=\"/stocks/list\""));
    assert!(body.contains("data-poll"));
    assert!(body.contains("키워드를 만드는 중…"));
    // 다 만들면 폴링을 멈춘다
    {
        let conn = web.writer();
        conn.execute("UPDATE stocks SET keywords_status = 'ready'", [])
            .unwrap();
    }
    let body = text(&web, "/stocks/list").await;
    assert!(!body.contains("hx-get=\"/stocks/list\""));
}

#[tokio::test]
async fn search_asks_the_worker_and_lists_candidates() {
    let worker = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/stocks/match"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "candidates": [{
                "market": "KR", "code": "005930", "name": "Samsung Electronics",
                "nameKo": "삼성전자", "exchange": "KOSPI", "isEtf": false, "alreadyRegistered": false
            }]
        })))
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    let (status, body) = post_form(&web, "/stocks/match", &[("query", "  삼전  ")]).await;
    assert_eq!(status, 200);
    assert!(body.contains("data-testid=\"candidate\""));
    assert!(body.contains("삼성전자"));
    assert!(body.contains("005930"));
    assert!(body.contains("KOSPI"));
    assert!(body.contains("aria-label=\"삼성전자 등록\""));
    // 검색 영역 전체를 바꾸므로 찾기 단추는 다시 "찾기"다
    assert!(body.contains("id=\"stock-search\""));
    assert!(body.contains("value=\"삼전\""));
}

#[tokio::test]
async fn search_with_no_hits_or_empty_input_says_so() {
    let worker = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/stocks/match"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "candidates": [] })))
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    let (_, body) = post_form(&web, "/stocks/match", &[("query", "없는종목이름")]).await;
    assert!(body.contains("맞는 종목을 찾지 못했습니다."));
    let (_, body) = post_form(&web, "/stocks/match", &[("query", "   ")]).await;
    assert!(body.contains("종목 이름이나 티커를 입력하세요."));
}

#[tokio::test]
async fn search_without_a_worker_says_it_is_unreachable() {
    let web = start_web(None).await;
    let (_, body) = post_form(&web, "/stocks/match", &[("query", "삼전")]).await;
    assert!(body.contains("worker에 연결할 수 없습니다."));
}

#[tokio::test]
async fn register_success_marks_the_candidate_and_refreshes_the_list() {
    let worker = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/stocks"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "ok": true })))
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    let (status, body) = post_form(
        &web,
        "/stocks/register",
        &[
            ("market", "KR"),
            ("code", "005930"),
            ("nameKo", "삼성전자"),
            ("name", "Samsung Electronics"),
            ("userInput", "삼전"),
        ],
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("등록함"));
    // 목록을 같은 응답에서 다시 그린다(Next `refresh()`)
    assert!(body.contains("id=\"stock-list\" hx-swap-oob=\"true\""));
    assert!(body.contains("id=\"cand-error-KR-005930\" class=\"hidden\" hx-swap-oob=\"true\""));
    let received = worker.received_requests().await.unwrap();
    let sent: serde_json::Value = serde_json::from_slice(&received[0].body).unwrap();
    assert_eq!(sent["market"], "KR");
    assert_eq!(sent["code"], "005930");
    assert_eq!(sent["userInput"], "삼전");
    assert_eq!(sent["nameKo"], "삼성전자");
}

#[tokio::test]
async fn register_of_a_known_stock_says_already_and_does_not_refresh() {
    let worker = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/stocks"))
        .respond_with(
            ResponseTemplate::new(409).set_body_json(json!({ "error": "already_registered" })),
        )
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    let (_, body) = post_form(
        &web,
        "/stocks/register",
        &[
            ("market", "KR"),
            ("code", "005930"),
            ("nameKo", ""),
            ("name", "삼성전자"),
            ("userInput", "삼전"),
        ],
    )
    .await;
    assert!(body.contains("이미 등록됨"));
    assert!(!body.contains("id=\"stock-list\""));
}

#[tokio::test]
async fn register_error_keeps_the_form_and_shows_the_reason_on_the_candidate() {
    let worker = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/stocks"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({ "error": "not_listed" })))
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    let (_, body) = post_form(
        &web,
        "/stocks/register",
        &[
            ("market", "KR"),
            ("code", "999999"),
            ("nameKo", "가짜"),
            ("name", "Fake"),
            ("userInput", "가짜"),
        ],
    )
    .await;
    assert!(body.contains("<form class=\"shrink-0\" hx-post=\"/stocks/register\""));
    assert!(body.contains(
        "id=\"cand-error-KR-999999\" role=\"alert\" class=\"mt-1 text-xs text-destructive\" hx-swap-oob=\"true\">공식 종목 목록에 없는 종목이라 등록할 수 없습니다.</p>"
    ));
}

#[tokio::test]
async fn delete_success_refreshes_the_list_and_missing_stock_counts_as_done() {
    let worker = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/stocks/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
        .mount(&worker)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/stocks/2"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({ "error": "not_found" })))
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    let (_, body) = post_form(&web, "/stocks/delete", &[("id", "1")]).await;
    assert!(body.contains("id=\"stock-list\" hx-swap-oob=\"true\""));
    let (_, body) = post_form(&web, "/stocks/delete", &[("id", "2")]).await;
    assert!(body.contains("id=\"stock-list\" hx-swap-oob=\"true\""));
}

#[tokio::test]
async fn delete_failure_stays_in_the_dialog() {
    let worker = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/stocks/3"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({ "error": "boom" })))
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    let (_, body) = post_form(&web, "/stocks/delete", &[("id", "3")]).await;
    assert!(body.contains(
        "id=\"delete-stock-error-3\" role=\"alert\" class=\"text-sm text-destructive\" hx-swap-oob=\"true\">처리하지 못했습니다. 다시 시도하세요.</p>"
    ));
    // 목록은 다시 그리지 않는다(확인창이 열린 채로 남는다)
    assert!(!body.contains("id=\"stock-list\""));
    assert!(body.contains("hx-post=\"/stocks/delete\""));
}

#[tokio::test]
async fn regenerate_queues_and_refreshes_and_failure_shows_under_the_button() {
    let worker = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/stocks/4/keywords"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({ "ok": true })))
        .mount(&worker)
        .await;
    Mock::given(method("POST"))
        .and(path("/stocks/5/keywords"))
        .respond_with(
            ResponseTemplate::new(503).set_body_json(json!({ "error": "worker_timeout" })),
        )
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    {
        let conn = web.writer();
        insert_stock(&conn, "000660", "SK하이닉스", "ready");
        conn.execute("UPDATE stocks SET id = 4 WHERE code = '000660'", [])
            .unwrap();
        insert_stock(&conn, "005380", "현대차", "failed");
        conn.execute("UPDATE stocks SET id = 5 WHERE code = '005380'", [])
            .unwrap();
    }
    let (_, body) = post_form(&web, "/stocks/keywords", &[("id", "4")]).await;
    assert!(body.contains("id=\"stock-list\" hx-swap-oob=\"true\""));
    let (_, body) = post_form(&web, "/stocks/keywords", &[("id", "5")]).await;
    assert!(body.contains("role=\"alert\" class=\"mt-1 text-xs text-destructive\">worker 응답이 늦습니다. 다시 시도하세요.</p>"));
    assert!(body.contains("aria-label=\"현대차 키워드 다시 만들기\""));
    assert!(body.contains("<form hx-post=\"/stocks/keywords\""));
}

#[tokio::test]
async fn market_toggle_refreshes_markets_and_topics() {
    let worker = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/markets/US"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    let (_, body) = post_form(
        &web,
        "/stocks/market",
        &[("market", "US"), ("enabled", "true")],
    )
    .await;
    assert!(body.contains("id=\"market-list\" hx-swap-oob=\"true\""));
    assert!(body.contains("id=\"topic-list\" hx-swap-oob=\"true\""));
}

#[tokio::test]
async fn topic_error_keeps_the_typed_name_and_says_why() {
    let worker = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/topics"))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({ "error": "exists" })))
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    let (_, body) = post_form(&web, "/stocks/topics", &[("name", "중동 정세")]).await;
    assert!(body.contains("value=\"중동 정세\""));
    assert!(body.contains("이미 넣은 주제입니다."));
    assert!(!body.contains("id=\"topic-list\" hx-swap-oob=\"true\""));
}

#[tokio::test]
async fn topic_added_clears_the_input_and_refreshes_the_list() {
    let worker = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/topics"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "ok": true })))
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    let (_, body) = post_form(&web, "/stocks/topics", &[("name", "부동산 시장")]).await;
    assert!(body.contains("value=\"\""));
    assert!(body.contains("id=\"topic-list\" hx-swap-oob=\"true\""));
}

#[tokio::test]
async fn stock_pages_need_a_login_for_posts_too() {
    let web = start_web(None).await;
    let response = web
        .client
        .post(web.url("/stocks/delete"))
        .body("id=1")
        .header("content-type", "application/x-www-form-urlencoded")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 403);
}

#[tokio::test]
async fn keyword_failure_reason_is_hidden_from_guests_only() {
    let web = start_web(None).await;
    {
        let conn = web.writer();
        insert_stock(&conn, "005380", "현대차", "failed");
        conn.execute(
            "UPDATE stocks SET keywords_error = 'SECRET-UPSTREAM-REASON' WHERE code = '005380'",
            [],
        )
        .unwrap();
    }
    let lead = "키워드를 만들지 못했습니다.";
    for path in ["/stocks", "/stocks/list"] {
        let guest = web
            .client
            .get(web.url(path))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(guest.contains(lead), "{path}");
        assert!(!guest.contains("SECRET-UPSTREAM-REASON"), "{path}");
        let member = text(&web, path).await;
        assert!(
            member.contains(&format!("{lead} SECRET-UPSTREAM-REASON")),
            "{path}"
        );
    }
}

#[tokio::test]
async fn guest_polls_slower_than_member() {
    let web = start_web(None).await;
    {
        let conn = web.writer();
        insert_stock(&conn, "005930", "삼성전자", "pending");
    }
    for path in ["/stocks", "/stocks/list"] {
        let guest = web
            .client
            .get(web.url(path))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(guest.contains("hx-trigger=\"every 10s\""), "{path}");
        assert!(!guest.contains("hx-trigger=\"every 3s\""), "{path}");
        let member = text(&web, path).await;
        assert!(member.contains("hx-trigger=\"every 3s\""), "{path}");
        assert!(!member.contains("hx-trigger=\"every 10s\""), "{path}");
    }
}
