use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

fn options(server: &MockServer) -> ModelListOptions {
    ModelListOptions {
        base_url: Some(server.uri()),
        timeout_ms: 5_000,
    }
}

// describe("isOpenRouterModel (2026-10-21 결정)")
#[tokio::test(flavor = "current_thread")]
async fn true_when_in_public_list_false_otherwise_and_follows_next_page() {
    // it("공개 모델 목록에 있으면 true, 없으면 false. 다음 쪽이 있으면 따라간다")
    let server = MockServer::start().await;
    let next = format!("{}/models?page=2", server.uri());
    Mock::given(method("GET"))
        .and(path("/models"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{ "id": "openai/gpt-x" }], "links": { "next": null }
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{ "id": "anthropic/claude-haiku-5.5" }], "links": { "next": next }
        })))
        .mount(&server)
        .await;
    let http = reqwest::Client::new();
    let check = |id: &'static str| {
        let http = http.clone();
        let opts = options(&server);
        async move { is_open_router_model(&http, id, &opts).await.unwrap() }
    };

    assert!(check("anthropic/claude-haiku-5.5").await);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    assert!(check("openai/gpt-x").await);
    assert!(!check("nobody/none").await);
}

#[tokio::test(flavor = "current_thread")]
async fn fails_with_model_list_error_when_list_is_unavailable_or_odd() {
    // it("목록을 받지 못하거나 형식이 다르면 ModelListError(저장하지 않음)")
    let http = reqwest::Client::new();
    let failing = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({})))
        .mount(&failing)
        .await;
    let error = is_open_router_model(&http, "a/b", &options(&failing))
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "OpenRouter 모델 목록 HTTP 503");

    let offline = ModelListOptions {
        base_url: Some("http://127.0.0.1:1".into()),
        timeout_ms: 5_000,
    };
    let error = is_open_router_model(&http, "a/b", &offline)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("OpenRouter 모델 목록을 받지 못했습니다"),
        "{error}"
    );

    let odd = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "models": [] })))
        .mount(&odd)
        .await;
    let error = is_open_router_model(&http, "a/b", &options(&odd))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("형식이 다릅니다"), "{error}");
}
