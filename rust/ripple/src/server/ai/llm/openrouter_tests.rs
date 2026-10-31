use std::time::Duration;

use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::server::ai::llm::{LlmMessage, StructuredResult, generate_object};

#[derive(Debug, PartialEq)]
struct Answer {
    answer: String,
}

fn parse_answer(value: &Value) -> Option<Answer> {
    Some(Answer {
        answer: value.get("answer")?.as_str()?.to_string(),
    })
}

/// TS 테스트의 zod 스키마 `z.object({ answer: z.string() })`가 만드는 JSON Schema(`$schema`가 붙어 옴)
fn answer_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "answer": { "type": "string" } },
        "required": ["answer"],
        "additionalProperties": false,
    })
}

fn request() -> StructuredRequest<Answer> {
    StructuredRequest {
        model: "anthropic/claude-haiku-5.5".to_string(),
        messages: vec![LlmMessage::system("시스템"), LlmMessage::user("질문")],
        schema: answer_schema(),
        schema_name: "answer".to_string(),
        max_tokens: 300,
        timeout_ms: None,
        parse: parse_answer,
    }
}

fn ok_body(content: &str) -> Value {
    json!({
        "model": "anthropic/claude-haiku-5.5",
        "choices": [{ "message": { "content": content } }],
        "usage": { "prompt_tokens": 120, "completion_tokens": 30, "cost": 0.000027 },
    })
}

async fn server_with(response: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(response)
        .mount(&server)
        .await;
    server
}

fn client(server: &MockServer, api_key: Option<&str>) -> OpenRouterClient {
    OpenRouterClient::new(
        api_key.map(str::to_string),
        Some(server.uri()),
        reqwest::Client::new(),
    )
}

async fn expect_llm_error(server: &MockServer, kind: LlmErrorKind) -> LlmError {
    let error = generate_object(&client(server, Some("sk-test")), &request())
        .await
        .unwrap_err();
    assert_eq!(error.kind, kind, "{error}");
    error
}

// describe("OpenRouter 클라이언트")
#[tokio::test(flavor = "current_thread")]
async fn sends_strict_json_schema_and_provider_condition() {
    // it("구조화 출력(strict JSON Schema)과 공급자 조건을 담아 요청한다")
    let server =
        server_with(ResponseTemplate::new(200).set_body_json(ok_body(r#"{"answer":"네"}"#))).await;
    generate_object(&client(&server, Some("sk-test")), &request())
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    let call = &requests[0];
    assert_eq!(call.url.path(), "/chat/completions");
    assert_eq!(
        call.headers.get("authorization").unwrap().to_str().unwrap(),
        "Bearer sk-test"
    );
    let body: Value = serde_json::from_slice(&call.body).unwrap();
    assert_eq!(body["model"], "anthropic/claude-haiku-5.5");
    assert_eq!(
        body["messages"],
        json!([
            { "role": "system", "content": "시스템" },
            { "role": "user", "content": "질문" },
        ])
    );
    assert_eq!(body["max_tokens"], 300);
    assert_eq!(body["provider"], json!({ "require_parameters": true }));
    assert_eq!(
        body["response_format"],
        json!({
            "type": "json_schema",
            "json_schema": {
                "name": "answer",
                "strict": true,
                "schema": {
                    "type": "object",
                    "properties": { "answer": { "type": "string" } },
                    "required": ["answer"],
                    "additionalProperties": false,
                },
            },
        })
    );
    assert!(
        body["response_format"]["json_schema"]["schema"]
            .get("$schema")
            .is_none()
    );
}

#[test]
fn default_base_url_is_official_address() {
    // it(...)의 URL 확인 부분: 기본 주소는 공식 OpenRouter다(실제로 부르지 않고 값만 확인).
    let client = OpenRouterClient::new(Some("k".into()), None, reqwest::Client::new());
    assert_eq!(
        format!("{}/chat/completions", client.base_url),
        "https://openrouter.ai/api/v1/chat/completions"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn returns_validated_value_with_usage_and_cost() {
    // it("응답을 검증해 돌려주고, 사용량·청구액(usage.cost)을 함께 준다")
    let server =
        server_with(ResponseTemplate::new(200).set_body_json(ok_body(r#"{"answer":"네"}"#))).await;
    let result = generate_object(&client(&server, Some("sk-test")), &request())
        .await
        .unwrap();
    assert_eq!(
        result,
        StructuredResult {
            value: Answer {
                answer: "네".into()
            },
            usage: LlmUsage {
                input_tokens: 120,
                output_tokens: 30,
                cost_usd: 0.000027
            },
            model: "anthropic/claude-haiku-5.5".into(),
        }
    );
}

#[tokio::test(flavor = "current_thread")]
async fn reads_json_wrapped_in_code_block() {
    // it("코드 블록으로 감싼 JSON도 읽는다")
    let server = server_with(
        ResponseTemplate::new(200).set_body_json(ok_body("```json\n{\"answer\":\"네\"}\n```")),
    )
    .await;
    let result = generate_object(&client(&server, Some("sk-test")), &request())
        .await
        .unwrap();
    assert_eq!(
        result.value,
        Answer {
            answer: "네".into()
        }
    );
}

#[tokio::test(flavor = "current_thread")]
async fn wrong_shape_is_invalid_output_with_billed_usage() {
    // it("형식이 다르면 invalid_output이고, 청구된 사용량을 오류에 담는다")
    let server =
        server_with(ResponseTemplate::new(200).set_body_json(ok_body(r#"{"wrong":1}"#))).await;
    let error = expect_llm_error(&server, LlmErrorKind::InvalidOutput).await;
    assert_eq!(error.usage.unwrap().cost_usd, 0.000027);
}

#[tokio::test(flavor = "current_thread")]
async fn http_status_maps_to_error_kind() {
    // it.each([[402, "payment_required"], [429, "rate_limited"], [500, "upstream"]])("HTTP %i → %s")
    for (status, kind) in [
        (402, LlmErrorKind::PaymentRequired),
        (429, LlmErrorKind::RateLimited),
        (500, LlmErrorKind::Upstream),
    ] {
        let server = server_with(
            ResponseTemplate::new(status)
                .set_body_json(json!({ "error": { "message": "x", "code": status } })),
        )
        .await;
        expect_llm_error(&server, kind).await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn error_body_with_status_200_is_failure() {
    // it("200이어도 본문에 error가 있으면 실패로 본다")
    let server = server_with(
        ResponseTemplate::new(200)
            .set_body_json(json!({ "error": { "message": "provider error", "code": 502 } })),
    )
    .await;
    let error = expect_llm_error(&server, LlmErrorKind::Upstream).await;
    assert_eq!(error.message, "OpenRouter 오류: provider error");
}

#[tokio::test(flavor = "current_thread")]
async fn error_messages_are_korean_sentences_like_ts() {
    // Rust에서만 필요한 점검: 화면·로그에 나가는 오류 문장이 TS와 같다.
    let server = server_with(
        ResponseTemplate::new(402)
            .set_body_json(json!({ "error": { "message": "no credits", "code": 402 } })),
    )
    .await;
    assert_eq!(
        expect_llm_error(&server, LlmErrorKind::PaymentRequired)
            .await
            .message,
        "OpenRouter 크레딧이 부족합니다: no credits"
    );
    // 본문이 없으면 상태 번호가 code가 되고 상세는 `HTTP <상태>`
    let server = server_with(ResponseTemplate::new(429)).await;
    assert_eq!(
        expect_llm_error(&server, LlmErrorKind::RateLimited)
            .await
            .message,
        "OpenRouter 요청 한도에 걸렸습니다: HTTP 429"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn slow_response_is_timeout() {
    // it("제한 시간이 지나면 timeout")
    let server = server_with(
        ResponseTemplate::new(200)
            .set_body_json(ok_body("{}"))
            .set_delay(Duration::from_millis(500)),
    )
    .await;
    let slow = client(&server, Some("sk-test")).with_timeout_ms(50);
    let error = generate_object(&slow, &request()).await.unwrap_err();
    assert_eq!(error.kind, LlmErrorKind::Timeout);
    assert_eq!(error.message, "OpenRouter 응답이 늦어 중단했습니다.");
}

#[tokio::test(flavor = "current_thread")]
async fn request_timeout_overrides_client_default() {
    // it("요청에 timeoutMs가 있으면 그 요청만 그만큼 기다린다")
    // 클라이언트 기본(50ms)보다 늦게, 요청의 제한 시간(1000ms)보다 먼저 답한다.
    let server = server_with(
        ResponseTemplate::new(200)
            .set_body_json(ok_body(r#"{"answer":"늦은 답"}"#))
            .set_delay(Duration::from_millis(200)),
    )
    .await;
    let slow = client(&server, Some("sk-test")).with_timeout_ms(50);
    let error = generate_object(&slow, &request()).await.unwrap_err();
    assert_eq!(error.kind, LlmErrorKind::Timeout);
    let result = generate_object(
        &slow,
        &StructuredRequest {
            timeout_ms: Some(1000),
            ..request()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        result.value,
        Answer {
            answer: "늦은 답".into()
        }
    );
}

#[tokio::test(flavor = "current_thread")]
async fn missing_api_key_makes_no_request() {
    // it("API 키가 없으면 요청하지 않고 no_api_key")
    let server = server_with(ResponseTemplate::new(200).set_body_json(ok_body("{}"))).await;
    let error = generate_object(&client(&server, None), &request())
        .await
        .unwrap_err();
    assert_eq!(error.kind, LlmErrorKind::NoApiKey);
    assert_eq!(error.message, "OPENROUTER_API_KEY가 설정되지 않았습니다.");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn connection_failure_is_upstream() {
    // Rust에서만 필요한 점검: 연결하지 못하면 upstream(TS의 fetch 실패와 같은 분류).
    let llm = OpenRouterClient::new(
        Some("k".into()),
        Some("http://127.0.0.1:1".into()),
        reqwest::Client::new(),
    );
    let error = generate_object(&llm, &request()).await.unwrap_err();
    assert_eq!(error.kind, LlmErrorKind::Upstream);
    assert!(
        error
            .message
            .starts_with("OpenRouter에 연결하지 못했습니다: ")
    );
}
