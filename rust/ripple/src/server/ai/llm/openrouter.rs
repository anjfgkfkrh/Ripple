use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use serde_json::{Value, json};

use crate::server::ai::llm::{
    JsonResult, LlmClient, LlmError, LlmErrorKind, LlmUsage, StructuredRequest,
};
use crate::server::db::schema::UsageProvider;

pub const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_RETRY_DELAY_MS: u64 = 5_000;

/// OpenRouter Chat Completions를 라이브러리 없이 직접 부른다.
/// - 구조화 출력(response_format: json_schema, strict)으로 JSON을 받고 호출한 쪽이 한 번 더 검증한다.
/// - require_parameters: 요청의 파라미터를 모두 지원하는 공급자에게만 보낸다.
/// - 사용량·청구액은 응답의 usage에 항상 들어 있다(OpenRouter Usage Accounting).
pub struct OpenRouterClient {
    api_key: Option<String>,
    base_url: String,
    timeout_ms: u64,
    /// 연결·시간 초과·본문 읽기 실패 뒤 다시 보내기 전에 기다리는 시간
    retry_delay_ms: u64,
    http: reqwest::Client,
}

impl OpenRouterClient {
    pub fn new(api_key: Option<String>, base_url: Option<String>, http: reqwest::Client) -> Self {
        Self {
            api_key,
            base_url: base_url.unwrap_or_else(|| OPENROUTER_BASE_URL.to_string()),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            retry_delay_ms: DEFAULT_RETRY_DELAY_MS,
            http,
        }
    }

    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    pub fn with_retry_delay_ms(mut self, retry_delay_ms: u64) -> Self {
        self.retry_delay_ms = retry_delay_ms;
        self
    }

    /// 연결 오류·시간 초과·본문 읽기 실패만 한 번 다시 보낸다. 형식 오류·HTTP 오류(429·402 포함)는 그대로 돌려준다.
    async fn send_with_retry<T: Send>(
        &self,
        api_key: &str,
        request: &StructuredRequest<T>,
        body: &Value,
    ) -> Result<JsonResult, LlmError> {
        match self.send_once(api_key, request, body).await {
            Err(failure) if failure.retryable => {
                tokio::time::sleep(Duration::from_millis(self.retry_delay_ms)).await;
                self.send_once(api_key, request, body)
                    .await
                    .map_err(|failure| failure.error)
            }
            other => other.map_err(|failure| failure.error),
        }
    }
}

/// 한 번 보낸 실패. `retryable`이면 전송 단계(연결·시간 초과·본문 읽기)에서 난 것이다.
struct SendFailure {
    error: LlmError,
    retryable: bool,
}

impl SendFailure {
    fn transport(error: LlmError) -> Self {
        Self {
            error,
            retryable: true,
        }
    }

    fn final_error(error: LlmError) -> Self {
        Self {
            error,
            retryable: false,
        }
    }
}

impl LlmClient for OpenRouterClient {
    fn provider(&self) -> UsageProvider {
        UsageProvider::Openrouter
    }

    async fn generate_json<T: Send>(
        &self,
        request: &StructuredRequest<T>,
    ) -> Result<JsonResult, LlmError> {
        let Some(api_key) = &self.api_key else {
            return Err(LlmError::new(
                "OPENROUTER_API_KEY가 설정되지 않았습니다.",
                LlmErrorKind::NoApiKey,
            ));
        };

        // 메타 필드($schema)를 받지 않는 공급자가 있어 뺀다.
        let mut schema = request.schema.clone();
        if let Some(object) = schema.as_object_mut() {
            object.shift_remove("$schema");
        }
        let messages: Vec<Value> = request
            .messages
            .iter()
            .map(|m| json!({ "role": m.role, "content": m.content }))
            .collect();
        let mut body = json!({
            "model": request.model,
            "messages": messages,
            "max_tokens": request.max_tokens,
            "response_format": {
                "type": "json_schema",
                "json_schema": { "name": request.schema_name, "strict": true, "schema": schema },
            },
            "provider": { "require_parameters": true },
        });
        if let Some(reasoning) = request.reasoning.body() {
            body["reasoning"] = reasoning;
        }
        self.send_with_retry(api_key, request, &body).await
    }
}

impl OpenRouterClient {
    async fn send_once<T: Send>(
        &self,
        api_key: &str,
        request: &StructuredRequest<T>,
        body: &Value,
    ) -> Result<JsonResult, SendFailure> {
        let timeout = Duration::from_millis(request.timeout_ms.unwrap_or(self.timeout_ms));
        let response = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(api_key)
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .timeout(timeout)
            .send()
            .await
            .map_err(|error| {
                SendFailure::transport(if error.is_timeout() {
                    timeout_error()
                } else {
                    LlmError::new(
                        format!("OpenRouter에 연결하지 못했습니다: {}", describe(&error)),
                        LlmErrorKind::Upstream,
                    )
                })
            })?;

        let status = response.status();
        // 본문을 읽지 못하면(끊김·시간 초과) 형식 오류가 아니라 연결 오류다. JSON이 아닌 본문은 None.
        let bytes = response.bytes().await.map_err(|error| {
            SendFailure::transport(if error.is_timeout() {
                timeout_error()
            } else {
                LlmError::new(
                    format!(
                        "OpenRouter 응답 본문을 읽지 못했습니다: {}",
                        describe(&error)
                    ),
                    LlmErrorKind::Upstream,
                )
            })
        })?;
        let payload: Option<Value> = serde_json::from_slice(&bytes).ok();
        if !status.is_success() || is_truthy(payload.as_ref().and_then(|p| p.get("error"))) {
            return Err(SendFailure::final_error(to_http_error(
                status.as_u16(),
                payload.as_ref(),
            )));
        }

        let usage = read_usage(payload.as_ref());
        let content = payload
            .as_ref()
            .and_then(|p| p.pointer("/choices/0/message/content"))
            .and_then(Value::as_str);
        let model = payload
            .as_ref()
            .and_then(|p| p.get("model"))
            .and_then(Value::as_str)
            .map_or_else(|| request.model.clone(), str::to_string);
        Ok(JsonResult {
            json: parse_json(content),
            usage,
            model,
        })
    }
}

fn timeout_error() -> LlmError {
    LlmError::new(
        "OpenRouter 응답이 늦어 중단했습니다.",
        LlmErrorKind::Timeout,
    )
}

/// JS 진리값: null·false·0·""·없음은 거짓
fn is_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

fn read_usage(payload: Option<&Value>) -> LlmUsage {
    let number = |key: &str| {
        payload
            .and_then(|p| p.get("usage"))
            .and_then(|u| u.get(key))
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    };
    LlmUsage {
        input_tokens: number("prompt_tokens") as i64,
        output_tokens: number("completion_tokens") as i64,
        cost_usd: number("cost"),
    }
}

static FENCE_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^```(?:json)?[\s\u{FEFF}]*").expect("정규식이 올바르다"));
static FENCE_END: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\s\u{FEFF}]*```$").expect("정규식이 올바르다"));

/// 대부분 순수 JSON이지만, 코드 블록으로 감싸 오는 모델에 대비한다.
fn parse_json(content: Option<&str>) -> Option<Value> {
    let content = content.filter(|c| !c.is_empty())?;
    let trimmed = crate::server::text::js_trim(content);
    let text = FENCE_START.replace(trimmed, "");
    let text = FENCE_END.replace(&text, "");
    serde_json::from_str(&text).ok()
}

fn to_http_error(status: u16, payload: Option<&Value>) -> LlmError {
    let error = payload.and_then(|p| p.get("error"));
    let detail = error
        .and_then(|e| e.get("message"))
        .and_then(Value::as_str)
        .map_or_else(|| format!("HTTP {status}"), str::to_string);
    // `payload?.error?.code ?? status`: 숫자 code만 같은 값으로 본다(엄격 비교).
    let code = error
        .and_then(|e| e.get("code"))
        .filter(|c| !c.is_null())
        .map_or(Some(f64::from(status)), Value::as_f64);
    match code {
        Some(402.0) => LlmError::new(
            format!("OpenRouter 크레딧이 부족합니다: {detail}"),
            LlmErrorKind::PaymentRequired,
        ),
        Some(429.0) => LlmError::new(
            format!("OpenRouter 요청 한도에 걸렸습니다: {detail}"),
            LlmErrorKind::RateLimited,
        ),
        _ => LlmError::new(format!("OpenRouter 오류: {detail}"), LlmErrorKind::Upstream),
    }
}

fn describe(error: &reqwest::Error) -> String {
    // reqwest는 원인을 source로 감싼다. 가장 안쪽 원인까지 붙여 읽을 수 있게 한다.
    let mut text = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(inner) = source {
        text.push_str(": ");
        text.push_str(&inner.to_string());
        source = inner.source();
    }
    text
}

#[cfg(test)]
#[path = "openrouter_tests.rs"]
mod tests;
