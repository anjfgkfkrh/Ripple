use std::time::Duration;

use serde_json::Value;

use crate::server::ai::llm::openrouter::OPENROUTER_BASE_URL;

/// 모델 목록을 확인하지 못했을 때(네트워크·응답 형식)
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ModelListError(pub String);

const MAX_PAGES: usize = 10;

pub struct ModelListOptions {
    pub base_url: Option<String>,
    pub timeout_ms: u64,
}

impl Default for ModelListOptions {
    fn default() -> Self {
        Self {
            base_url: None,
            timeout_ms: 15_000,
        }
    }
}

/// OpenRouter에 이 모델 ID가 있는지 공개 모델 목록(GET /models, 키 불필요·무료)으로 확인한다
/// (설정 화면 저장 때, 2026-10-21 결정). 2026-10-21 실측: { data: [{ id, … }], links: { next } }.
pub async fn is_open_router_model(
    http: &reqwest::Client,
    model_id: &str,
    options: &ModelListOptions,
) -> Result<bool, ModelListError> {
    let base = options.base_url.as_deref().unwrap_or(OPENROUTER_BASE_URL);
    let mut url = Some(format!("{base}/models"));
    let mut page = 0;
    while let Some(current) = url.take() {
        if page >= MAX_PAGES {
            break;
        }
        page += 1;
        let body = fetch_page(http, &current, options.timeout_ms).await?;
        let Some(data) = body.get("data").and_then(Value::as_array) else {
            return Err(ModelListError(
                "OpenRouter 모델 목록 형식이 다릅니다.".to_string(),
            ));
        };
        if data
            .iter()
            .any(|model| model.get("id").and_then(Value::as_str) == Some(model_id))
        {
            return Ok(true);
        }
        url = body
            .pointer("/links/next")
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    Ok(false)
}

async fn fetch_page(
    http: &reqwest::Client,
    url: &str,
    timeout_ms: u64,
) -> Result<Value, ModelListError> {
    let failed = |message: String| {
        ModelListError(format!("OpenRouter 모델 목록을 받지 못했습니다: {message}"))
    };
    let response = http
        .get(url)
        .timeout(Duration::from_millis(timeout_ms))
        .send()
        .await
        .map_err(|e| failed(e.to_string()))?;
    if !response.status().is_success() {
        return Err(ModelListError(format!(
            "OpenRouter 모델 목록 HTTP {}",
            response.status().as_u16()
        )));
    }
    response
        .json::<Value>()
        .await
        .map_err(|e| failed(e.to_string()))
}

#[cfg(test)]
#[path = "models_tests.rs"]
mod tests;
