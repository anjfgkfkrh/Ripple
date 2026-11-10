use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

const TOKEN: &str = "client-test-token-0123456789abcdef";

fn api(server: &MockServer) -> WorkerApi {
    WorkerApi::new(reqwest::Client::new(), server.uri(), TOKEN.to_string())
}

// describe("web → worker 내부 API") — it("토큰을 붙여 찾기·등록·키워드 다시 만들기·삭제를 부른다")
#[tokio::test]
async fn sends_token_and_bodies_for_stock_calls() {
    let server = MockServer::start().await;
    let auth = format!("Bearer {TOKEN}");
    Mock::given(method("POST"))
        .and(path("/stocks/match"))
        .and(header("authorization", auth.as_str()))
        .and(body_json(json!({"query": "NVDA"})))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"candidates": [{"code": "NVDA"}]})),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/stocks"))
        .and(body_json(
            json!({"market": "US", "code": "NVDA", "userInput": "NVDA", "nameKo": "엔비디아"}),
        ))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_json(json!({"status": "created", "stock": {"id": 7}})),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/stocks/7/keywords"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({"status": "queued"})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/stocks/7/aliases/remove"))
        .and(body_json(json!({"alias": "삼전"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/stocks/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&server)
        .await;
    let api = api(&server);

    let matched = api.match_stocks("NVDA").await;
    assert_eq!(
        matched,
        WorkerResult::Ok {
            status: 200,
            data: json!({"candidates": [{"code": "NVDA"}]})
        }
    );
    let registered = api
        .register_stock(
            json!({"market": "US", "code": "NVDA", "userInput": "NVDA", "nameKo": "엔비디아"}),
        )
        .await;
    assert_eq!(
        registered,
        WorkerResult::Ok {
            status: 201,
            data: json!({"status": "created", "stock": {"id": 7}})
        }
    );
    assert_eq!(
        api.regenerate_keywords(7).await,
        WorkerResult::Ok {
            status: 202,
            data: json!({"status": "queued"})
        }
    );
    assert!(api.remove_alias(7, "삼전").await.is_ok());
    assert!(api.delete_stock(7).await.is_ok());
}

// it("worker가 거절하면 오류 코드와 본문을 그대로 돌려준다")
#[tokio::test]
async fn passes_through_error_code_and_body() {
    let server = MockServer::start().await;
    Mock::given(path("/stocks/match"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"error": "unauthorized"})))
        .mount(&server)
        .await;
    assert_eq!(
        api(&server).match_stocks("삼전").await,
        WorkerResult::Err {
            status: 401,
            error: "unauthorized".to_string(),
            body: json!({"error": "unauthorized"})
        }
    );
}

// it("worker가 꺼져 있으면 worker_unreachable")
#[tokio::test]
async fn unreachable_worker() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let api = WorkerApi::new(
        reqwest::Client::new(),
        format!("http://127.0.0.1:{port}"),
        TOKEN.to_string(),
    );
    assert_eq!(
        api.delete_stock(1).await,
        WorkerResult::Err {
            status: 0,
            error: "worker_unreachable".to_string(),
            body: json!({})
        }
    );
}

// it("카드에 묶인 기사들을 읽음·안 읽음으로 저장한다(12단계)")
#[tokio::test]
async fn saves_read_state() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/articles/read"))
        .and(body_json(json!({"articleIds": [1, 2], "read": true})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"changed": 0})))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/articles/read"))
        .and(body_json(json!({"articleIds": [], "read": true})))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({"error": "invalid_request"})))
        .mount(&server)
        .await;
    let api = api(&server);
    assert_eq!(
        api.set_articles_read(&[1, 2], true).await,
        WorkerResult::Ok {
            status: 200,
            data: json!({"changed": 0})
        }
    );
    assert!(matches!(
        api.set_articles_read(&[], true).await,
        WorkerResult::Err { status: 400, .. }
    ));
}

// it("시장 켜기·주제 넣기·지우기를 부르고, 토픽 갱신이 없으면 503을 그대로 돌려준다(11단계)")
#[tokio::test]
async fn market_topic_calls_and_unavailable_refresh() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/markets/US"))
        .and(body_json(json!({"enabled": true})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/topics"))
        .and(body_json(json!({"name": "부동산 시장"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"topicId": 3})))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/topics"))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({"error": "exists"})))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/topics/3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/topics/refresh"))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({"error": "unavailable"})))
        .mount(&server)
        .await;
    let api = api(&server);

    assert!(api.set_market("US", true).await.is_ok());
    assert!(matches!(
        api.add_topic("부동산 시장").await,
        WorkerResult::Ok { status: 201, .. }
    ));
    assert!(matches!(
        api.add_topic("부동산 시장").await,
        WorkerResult::Err { status: 409, ref error, .. } if error == "exists"
    ));
    assert!(api.delete_topic(3).await.is_ok());
    assert!(matches!(
        api.refresh_topics().await,
        WorkerResult::Err { status: 503, ref error, .. } if error == "unavailable"
    ));
}

// Rust 전용: 응답이 늦으면 worker_timeout (TS의 TimeoutError)
#[tokio::test]
async fn slow_response_is_worker_timeout() {
    let server = MockServer::start().await;
    Mock::given(path("/collect"))
        .respond_with(ResponseTemplate::new(202).set_delay(std::time::Duration::from_secs(3)))
        .mount(&server)
        .await;
    let result = api(&server).call("/collect", Method::POST, None, 200).await;
    assert!(
        matches!(result, WorkerResult::Err { status: 0, ref error, .. } if error == "worker_timeout")
    );
}

// Rust 전용: 본문이 JSON이 아닌 오류는 http_<상태>
#[tokio::test]
async fn non_json_error_uses_http_status_code() {
    let server = MockServer::start().await;
    Mock::given(path("/collect"))
        .respond_with(ResponseTemplate::new(502).set_body_string("bad gateway"))
        .mount(&server)
        .await;
    assert_eq!(
        api(&server).start_collection().await,
        WorkerResult::Err {
            status: 502,
            error: "http_502".to_string(),
            body: json!({})
        }
    );
}
