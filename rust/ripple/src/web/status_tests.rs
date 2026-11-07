use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::web::test_support::{T0, start_web, start_web_with_tunnel};

#[test]
fn next_text_forms() {
    assert_eq!(next_text(T0 - 1, T0), "곧 시작");
    assert_eq!(next_text(T0 + 60_000, T0), "다음 09:01");
    assert_eq!(next_text(T0 + 2 * 86_400_000, T0), "다음 10-21 09:00");
}

#[tokio::test]
async fn status_page_has_sections_and_robots() {
    let web = start_web(None).await;
    let body = web
        .get("/status")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    for part in [
        "시스템 상태",
        "확인 중…",
        "data-testid=\"collection\"",
        "data-testid=\"topic-refresh\"",
        "noindex, nofollow",
        "아직 수집하지 않았습니다.",
    ] {
        assert!(body.contains(part), "{part}");
    }
}

#[tokio::test]
async fn system_partial_shows_db_and_worker() {
    let worker = MockServer::start().await;
    Mock::given(path("/health"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"status": "ok", "listings": {"KR": 7, "US": 9}})),
        )
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    let body = web
        .get("/status/system")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("journal_mode"));
    assert!(body.contains("응답함 · 공식 종목 목록 한국 7개, 미국 9개"));
    assert!(body.contains("터널을 쓰지 않습니다(이 컴퓨터에서만 접속)."));
    assert!(body.contains("아직 받은 적이 없습니다"));
}

async fn fixed_system_body(
    ready: Option<ResponseTemplate>,
    metrics: bool,
    public_url: &str,
) -> String {
    let tunnel = MockServer::start().await;
    if let Some(ready) = ready {
        Mock::given(method("GET"))
            .and(path("/ready"))
            .respond_with(ready)
            .mount(&tunnel)
            .await;
    }
    let web = start_web_with_tunnel(
        None,
        metrics.then(|| tunnel.uri()),
        Some(public_url.to_string()),
    )
    .await;
    web.get("/status/system")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap()
}

#[tokio::test]
async fn fixed_address_mode_shows_link_and_connection() {
    let body = fixed_system_body(
        Some(
            ResponseTemplate::new(200).set_body_json(json!({"status": 200, "readyConnections": 4})),
        ),
        true,
        "https://example.com/",
    )
    .await;
    assert!(body.contains("외부 접속(고정 주소)"));
    assert!(!body.contains("외부 접속(임시 주소)"));
    assert!(body.contains("href=\"https://example.com\""));
    assert!(body.contains("휴대폰 브라우저에 이 주소를 넣으세요."));
    assert!(!body.contains("다시 켜면 주소가 바뀝니다"));
    assert!(body.contains("연결됨(연결 4개)"));

    let plain = fixed_system_body(
        Some(ResponseTemplate::new(200)),
        true,
        "https://example.com",
    )
    .await;
    assert!(plain.contains("연결됨"));
    assert!(!plain.contains("연결됨(연결"));
}

#[tokio::test]
async fn fixed_address_mode_shows_disconnected_unreachable_unknown_and_bad_format() {
    let down = fixed_system_body(
        Some(ResponseTemplate::new(503)),
        true,
        "https://example.com",
    )
    .await;
    assert!(down.contains("Cloudflare와 연결되지 않았습니다"));
    assert!(down.contains("href=\"https://example.com\""));

    // /ready 응답을 등록하지 않으면 wiremock이 404를 준다. 닿지 않는 경우는 막힌 포트로 만든다.
    let web = start_web_with_tunnel(
        None,
        Some("http://127.0.0.1:1".to_string()),
        Some("https://example.com".to_string()),
    )
    .await;
    let body = web
        .get("/status/system")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("터널에 닿지 못했습니다: "));

    let unknown = fixed_system_body(None, false, "https://example.com").await;
    assert!(unknown.contains("연결 상태를 알 수 없습니다(터널 메트릭 주소 없음)"));
    assert!(unknown.contains("href=\"https://example.com\""));

    let bad = fixed_system_body(None, false, "http://example.com").await;
    assert!(bad.contains("PUBLIC_URL 형식이 다릅니다(https://호스트 꼴)"));
    assert!(!bad.contains("href=\"http://example.com"));
}

#[tokio::test]
async fn collect_now_starts_and_maps_errors() {
    let worker = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/collect"))
        .respond_with(
            ResponseTemplate::new(409)
                .set_body_json(json!({"error": "already_running", "runId": 1})),
        )
        .up_to_n_times(1)
        .mount(&worker)
        .await;
    Mock::given(method("POST"))
        .and(path("/collect"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({"runId": 2})))
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    let first = web
        .post("/status/collect")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(first.contains("이미 수집 중입니다."));
    let second = web
        .post("/status/collect")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!second.contains("role=\"alert\""));
    let none = start_web(None).await;
    let text = none
        .post("/status/collect")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(text.contains("worker에 연결할 수 없습니다."));
}

#[tokio::test]
async fn running_panel_polls_and_finished_does_not() {
    let web = start_web(None).await;
    let conn = web.writer();
    conn.execute(
        "INSERT INTO collection_runs (trigger, status, started_at, since, sources) VALUES ('manual','running',?1,?1,'[]')",
        [T0 - 5000],
    ).unwrap();
    let running = web
        .get("/status/collection")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(running.contains("hx-trigger=\"every 3s\""));
    assert!(running.contains("수집 중…"));
    conn.execute("UPDATE collection_runs SET status='partial', finished_at=?1, sources=?2",
        rusqlite::params![T0, r#"[{"id":"a","label":"가 경제","group":"가","fetched":3,"fresh":2,"saved":1,"matched":0,"ms":1,"error":"HTTP 500"}]"#]).unwrap();
    let done = web
        .get("/status/collection")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!done.contains("every 3s"));
    assert!(done.contains("일부 실패"));
    assert!(done.contains("가 경제: HTTP 500"));
    assert!(done.contains("출처별 결과 1곳"));
}

#[tokio::test]
async fn topics_refresh_notice_when_not_started() {
    let worker = MockServer::start().await;
    Mock::given(path("/topics/refresh"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({"started": false})))
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    let text = web
        .post("/status/topics/refresh")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(text.contains("이미 갱신 중입니다."));
}

// Rust 전용: 동작 직후의 패널은 아직 '진행 중' 기록이 없어도 한 번 더 읽어 보도록 polling을 건다
#[tokio::test]
async fn panel_after_action_polls_once_more() {
    let worker = MockServer::start().await;
    Mock::given(path("/topics/refresh"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({"started": true})))
        .mount(&worker)
        .await;
    let web = start_web(Some(worker.uri())).await;
    let after = web
        .post("/status/topics/refresh")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(after.contains("hx-trigger=\"every 3s\""));
    let plain = web
        .get("/status/topics")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!plain.contains("every 3s"));
}
