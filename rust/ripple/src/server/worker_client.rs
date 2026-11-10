//! web → worker 내부 API 호출 (SPEC D-36: 쓰기와 AI 호출은 worker가 맡는다, `worker-client.ts`).
use std::time::Duration;

use reqwest::Method;
use serde_json::{Value, json};

use crate::server::tunnel::describe_request_error;

#[derive(Debug, Clone, PartialEq)]
pub enum WorkerHealth {
    Ok { listings: Option<(i64, i64)> },
    Err { reason: String },
}

pub async fn fetch_worker_health(
    http: &reqwest::Client,
    worker_url: &str,
    timeout_ms: u64,
) -> WorkerHealth {
    match request_health(http, worker_url, timeout_ms).await {
        Ok(health) | Err(health) => health,
    }
}

async fn request_health(
    http: &reqwest::Client,
    worker_url: &str,
    timeout_ms: u64,
) -> Result<WorkerHealth, WorkerHealth> {
    let fail = |reason: String| WorkerHealth::Err { reason };
    let url = url::Url::parse(worker_url)
        .and_then(|base| base.join("/health"))
        .map_err(|e| fail(e.to_string()))?;
    let response = http
        .get(url)
        .timeout(Duration::from_millis(timeout_ms))
        .send()
        .await
        .map_err(|e| fail(describe_request_error(&e)))?;
    if !response.status().is_success() {
        return Err(fail(format!("HTTP {}", response.status().as_u16())));
    }
    let body: Value = response
        .json()
        .await
        .map_err(|e| fail(describe_request_error(&e)))?;
    if body.get("status").and_then(Value::as_str) != Some("ok") {
        return Err(fail("응답 형식이 올바르지 않습니다.".to_string()));
    }
    let listings = body.get("listings");
    let count = |key: &str| listings?.get(key)?.as_f64().map(|v| v as i64);
    Ok(WorkerHealth::Ok {
        listings: count("KR").zip(count("US")),
    })
}

/// 실패하면 worker가 준 오류 코드(예: budget_exceeded)와 본문을 그대로 돌려준다.
#[derive(Debug, Clone, PartialEq)]
pub enum WorkerResult {
    Ok {
        status: u16,
        data: Value,
    },
    Err {
        status: u16,
        error: String,
        body: Value,
    },
}

impl WorkerResult {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok { .. })
    }
}

#[derive(Clone)]
pub struct WorkerApi {
    http: reqwest::Client,
    worker_url: String,
    token: String,
}

const SHORT_MS: u64 = 10_000;

impl WorkerApi {
    pub fn new(http: reqwest::Client, worker_url: String, token: String) -> Self {
        Self {
            http,
            worker_url,
            token,
        }
    }

    async fn call(
        &self,
        path: &str,
        method: Method,
        body: Option<Value>,
        timeout_ms: u64,
    ) -> WorkerResult {
        let unreachable = |error: &str| WorkerResult::Err {
            status: 0,
            error: error.to_string(),
            body: json!({}),
        };
        let Ok(url) = url::Url::parse(&self.worker_url).and_then(|base| base.join(path)) else {
            return unreachable("worker_unreachable");
        };
        let mut request = self
            .http
            .request(method, url)
            .bearer_auth(&self.token)
            .timeout(Duration::from_millis(timeout_ms));
        if let Some(body) = body {
            request = request.json(&body);
        } else {
            request = request.header("content-type", "application/json");
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) if error.is_timeout() => return unreachable("worker_timeout"),
            Err(_) => return unreachable("worker_unreachable"),
        };
        let status = response.status();
        let body: Value = match response.bytes().await {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})),
            Err(error) if error.is_timeout() => return unreachable("worker_timeout"),
            Err(_) => json!({}),
        };
        if !status.is_success() {
            let error = body
                .get("error")
                .and_then(Value::as_str)
                .map_or_else(|| format!("http_{}", status.as_u16()), str::to_string);
            return WorkerResult::Err {
                status: status.as_u16(),
                error,
                body,
            };
        }
        WorkerResult::Ok {
            status: status.as_u16(),
            data: body,
        }
    }

    /// LLM 응답을 기다리므로 넉넉하게 기다린다(worker의 OpenRouter 제한 30초보다 길게).
    pub async fn match_stocks(&self, query: &str) -> WorkerResult {
        self.call(
            "/stocks/match",
            Method::POST,
            Some(json!({ "query": query })),
            45_000,
        )
        .await
    }

    pub async fn register_stock(&self, input: Value) -> WorkerResult {
        self.call("/stocks", Method::POST, Some(input), SHORT_MS)
            .await
    }

    pub async fn delete_stock(&self, id: i64) -> WorkerResult {
        self.call(&format!("/stocks/{id}"), Method::DELETE, None, SHORT_MS)
            .await
    }

    /// 키워드 생성을 대기열에 넣는다. 결과는 기다리지 않는다(D-37).
    pub async fn regenerate_keywords(&self, id: i64) -> WorkerResult {
        self.call(
            &format!("/stocks/{id}/keywords"),
            Method::POST,
            None,
            SHORT_MS,
        )
        .await
    }

    /// 17단계: 종목의 별칭 하나를 지운다(없는 종목·별칭이면 404 `not_found`·`alias_not_found`).
    pub async fn remove_alias(&self, id: i64, alias: &str) -> WorkerResult {
        self.call(
            &format!("/stocks/{id}/aliases/remove"),
            Method::POST,
            Some(json!({ "alias": alias })),
            SHORT_MS,
        )
        .await
    }

    /// 기사 수집을 시작한다. 결과는 기다리지 않는다(이미 도는 중이면 already_running).
    pub async fn start_collection(&self) -> WorkerResult {
        self.call("/collect", Method::POST, None, SHORT_MS).await
    }

    /// 모델을 바꾸면 worker가 OpenRouter 목록(최대 15초)을 확인하므로 넉넉하게 기다린다.
    pub async fn update_settings(&self, values: Value) -> WorkerResult {
        self.call("/settings", Method::PUT, Some(values), 30_000)
            .await
    }

    pub async fn rebuild_keywords(&self) -> WorkerResult {
        self.call("/keywords/rebuild", Method::POST, None, SHORT_MS)
            .await
    }

    /// 11단계: 한국·미국 증시를 켜거나 끈다(켜면 키워드를 만들기 시작).
    pub async fn set_market(&self, market: &str, enabled: bool) -> WorkerResult {
        self.call(
            &format!("/markets/{market}"),
            Method::PUT,
            Some(json!({ "enabled": enabled })),
            SHORT_MS,
        )
        .await
    }

    pub async fn add_topic(&self, name: &str) -> WorkerResult {
        self.call(
            "/topics",
            Method::POST,
            Some(json!({ "name": name })),
            SHORT_MS,
        )
        .await
    }

    pub async fn delete_topic(&self, id: i64) -> WorkerResult {
        self.call(&format!("/topics/{id}"), Method::DELETE, None, SHORT_MS)
            .await
    }

    /// 11단계: 자동 토픽을 지금 갱신한다. 결과는 기다리지 않는다(이미 하고 있으면 started false).
    pub async fn refresh_topics(&self) -> WorkerResult {
        self.call("/topics/refresh", Method::POST, None, SHORT_MS)
            .await
    }

    /// 12단계: 카드에 묶인 기사들을 읽음/안 읽음으로 저장한다(changed: 바뀐 기사 수).
    pub async fn set_articles_read(&self, article_ids: &[i64], read: bool) -> WorkerResult {
        self.call(
            "/articles/read",
            Method::PUT,
            Some(json!({ "articleIds": article_ids, "read": read })),
            SHORT_MS,
        )
        .await
    }
}

#[cfg(test)]
#[path = "worker_client_tests.rs"]
mod tests;
