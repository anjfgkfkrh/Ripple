//! 생성형 LLM 인터페이스 (SPEC 6.2 ai/llm). MVP 구현은 OpenRouter.
use std::future::Future;

use serde_json::Value;

use crate::server::db::schema::UsageProvider;

pub mod models;
pub mod openrouter;
pub mod schemas;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmMessage {
    pub role: &'static str,
    pub content: String,
}

impl LlmMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system",
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user",
            content: content.into(),
        }
    }
}

/// 구조화 출력 요청. TS의 zod 스키마 대신 JSON Schema(`schema`)를 보내고, 응답은 `parse`로 검증한다.
pub struct StructuredRequest<T> {
    pub model: String,
    pub messages: Vec<LlmMessage>,
    /// 구조화 출력 JSON Schema(`schemas` 모듈의 TS가 내보낸 파일)
    pub schema: Value,
    /// 구조화 출력 스키마 이름(영문·숫자·밑줄)
    pub schema_name: String,
    pub max_tokens: u32,
    /// 이 요청만 응답을 더 오래 기다릴 때(밀리초). 없으면 클라이언트 기본값
    pub timeout_ms: Option<u64>,
    /// 응답 JSON이 정해진 형식이면 값으로 바꾼다(zod `safeParse` 대응). 틀리면 None
    pub parse: fn(&Value) -> Option<T>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LlmUsage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    /// 실제 청구액(달러)
    pub cost_usd: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructuredResult<T> {
    pub value: T,
    pub usage: LlmUsage,
    pub model: String,
}

/// 클라이언트가 돌려주는 날 응답: 읽은 JSON(없거나 깨졌으면 None)과 사용량·모델
#[derive(Debug, Clone, PartialEq)]
pub struct JsonResult {
    pub json: Option<Value>,
    pub usage: LlmUsage,
    pub model: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmErrorKind {
    NoApiKey,
    PaymentRequired,
    RateLimited,
    Timeout,
    Upstream,
    InvalidOutput,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{message}")]
pub struct LlmError {
    pub message: String,
    pub kind: LlmErrorKind,
    /// 응답을 받은 뒤 실패했으면 청구된 사용량(기록해야 함)
    pub usage: Option<LlmUsage>,
}

impl LlmError {
    pub fn new(message: impl Into<String>, kind: LlmErrorKind) -> Self {
        Self {
            message: message.into(),
            kind,
            usage: None,
        }
    }
}

/// 요청 하나를 보내 JSON을 받는 쪽(OpenRouter, 테스트의 가짜). 검증은 `generate_object`가 한다.
pub trait LlmClient: Send + Sync {
    fn provider(&self) -> UsageProvider;

    fn generate_json<T: Send>(
        &self,
        request: &StructuredRequest<T>,
    ) -> impl Future<Output = Result<JsonResult, LlmError>> + Send;
}

/// 정해진 스키마에 맞는 JSON을 받는다. 형식이 틀리면 청구된 사용량을 담은 `invalid_output`.
pub async fn generate_object<T: Send>(
    llm: &impl LlmClient,
    request: &StructuredRequest<T>,
) -> Result<StructuredResult<T>, LlmError> {
    let raw = llm.generate_json(request).await?;
    let parsed = raw.json.as_ref().and_then(|json| (request.parse)(json));
    match parsed {
        Some(value) => Ok(StructuredResult {
            value,
            usage: raw.usage,
            model: raw.model,
        }),
        None => Err(LlmError {
            message: "OpenRouter 응답이 정해진 형식과 다릅니다.".to_string(),
            kind: LlmErrorKind::InvalidOutput,
            usage: Some(raw.usage),
        }),
    }
}
