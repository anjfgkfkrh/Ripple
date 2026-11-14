use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::header::HeaderMap;
use serde_json::{Value, json};

use crate::server::ai::decider::questions::{
    RELEVANCE_SCALE, build_article_questions, build_article_state, build_lead_questions,
    build_lead_state, build_merged_questions, build_merged_state, market_key,
};
use crate::server::ai::decider::types::{
    Decider, DeciderError, DeciderErrorKind, Decision, DecisionInput, JudgeTarget, LeadInput,
    LeadRating, MarketVerdict, MergedDecision, MergedInput, Scored, TokenUsage,
};
use crate::server::db::schema::Impact;
use crate::server::text::js_parse_number;

pub const TYPESAFE_BASE_URL: &str = "https://api.typesafe.ai";

// 재시도·제한 시간은 공식 TS SDK 0.7.0의 기본값을 그대로 옮긴 것이다
// (`node_modules/@typesafe-ai/sdk/dist/index.mjs`의 DEFAULT_RETRY_POLICY·retryDelayMs·systemOne).
const MAX_RETRIES: i64 = 2;
const BACKOFF_INITIAL_MS: f64 = 500.0;
const BACKOFF_MAX_MS: f64 = 5000.0;
const BACKOFF_JITTER: f64 = 0.25;
/// 서버가 알려 준 대기가 이보다 길면 무시하고 지수 백오프를 쓴다.
const MAX_RETRY_AFTER_MS: f64 = 60_000.0;
/// 시도마다 제한 시간(응답 본문까지)
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(10);

/// HTTP 408, 429, 5xx만 다시 시도한다.
fn is_retryable_status(status: u16) -> bool {
    status == 408 || status == 429 || (500..600).contains(&status)
}

/// `retry-after-ms` 또는 `Retry-After`를 밀리초로. 둘 다 올바르지 않으면 None(`retry-after-ms` 우선).
fn parse_retry_after(headers: &HeaderMap, now_ms: i64) -> Option<f64> {
    let text = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    if let Some(raw) = headers.get("retry-after-ms") {
        let ms = js_parse_number(raw.to_str().unwrap_or(""));
        if ms.is_finite() && ms >= 0.0 {
            return Some(ms);
        }
    }
    let raw = text("retry-after")?;
    let seconds = js_parse_number(raw);
    if seconds.is_finite() {
        return (seconds >= 0.0).then_some(seconds * 1000.0);
    }
    // HTTP 날짜("Wed, 21 Oct 2015 07:28:00 GMT")
    let date = chrono::DateTime::parse_from_rfc2822(raw.trim()).ok()?;
    Some((date.timestamp_millis() - now_ms).max(0) as f64)
}

/// 다음 시도 전 기다릴 시간. 서버가 알려 준 대기를 쓰고, 없으면 지수 백오프에 지터를 더한다.
/// `random`은 0 이상 1 미만의 값(테스트에서 고정).
fn retry_delay_ms(attempt: u32, headers: Option<&HeaderMap>, now_ms: i64, random: f64) -> u64 {
    if let Some(retry_after) = headers.and_then(|h| parse_retry_after(h, now_ms))
        && retry_after <= MAX_RETRY_AFTER_MS
    {
        return retry_after as u64;
    }
    let exponential = (BACKOFF_INITIAL_MS * 2f64.powi(attempt as i32)).min(BACKOFF_MAX_MS);
    // JS Math.round: .5는 위로
    (exponential * (1.0 - random * BACKOFF_JITTER) + 0.5).floor() as u64
}

fn now_ms() -> i64 {
    crate::server::time::now_ms()
}

/// 지터용 0 이상 1 미만 값. 암호용이 아니라 대기 시간을 흩뜨리는 용도라 시계의 나노초면 충분하다.
fn jitter_random() -> f64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    f64::from(nanos % 1_000_000) / 1_000_000.0
}

/// 한 번의 전송 실패(재시도 대상)와 HTTP 오류
enum Failure {
    Timeout,
    Connection,
    Api { status: u16 },
}

/// TypeSafe Jev(System One API)를 직접 부른다(결정 14단계 R2: SDK 없이 reqwest).
/// 기사는 관련도·영향도·호재/악재를, 키워드·토픽은 관련도·영향도를 한 요청에 묻는다(입력은 한 번만 센다).
/// 제한 시간(시도마다 10초)과 재시도(408·429·5xx·연결 오류·시간 초과는 두 번 더)는 SDK 기본값과 같다.
pub struct TypeSafeDecider {
    api_key: Option<String>,
    base_url: String,
    http: reqwest::Client,
}

impl TypeSafeDecider {
    pub fn new(api_key: Option<String>, base_url: Option<String>, http: reqwest::Client) -> Self {
        let base = base_url.unwrap_or_else(|| TYPESAFE_BASE_URL.to_string());
        Self {
            api_key,
            base_url: base.trim_end_matches('/').to_string(),
            http,
        }
    }

    /// `POST {base}/v1/systemone`. 성공하면 읽은 본문(JSON이 아니면 문자열, 비었으면 Null)을 돌려준다.
    async fn system_one(
        &self,
        model: &str,
        state: Value,
        questions: Value,
    ) -> Result<Value, DeciderError> {
        let Some(api_key) = &self.api_key else {
            return Err(DeciderError::new(
                "TYPESAFE_API_KEY가 설정되지 않았습니다.",
                DeciderErrorKind::NoApiKey,
            ));
        };
        // 요청이 작아(수 KB) 압축 이득이 거의 없어 압축하지 않는다.
        let body = json!({ "model": model, "state": state, "questions": questions }).to_string();
        let url = format!("{}/v1/systemone", self.base_url);
        for attempt in 0u32.. {
            let retries_left = MAX_RETRIES - i64::from(attempt);
            let result = self.attempt(&url, api_key, &body, attempt).await;
            let (failure, headers) = match result {
                Ok(parsed) => return Ok(parsed),
                Err(pair) => pair,
            };
            let retryable = match failure {
                Failure::Timeout | Failure::Connection => true,
                Failure::Api { status } => is_retryable_status(status),
            };
            if retries_left <= 0 || !retryable {
                return Err(to_decider_error(&failure));
            }
            let delay = retry_delay_ms(attempt, headers.as_ref(), now_ms(), jitter_random());
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        unreachable!("재시도는 MAX_RETRIES에서 끝난다")
    }

    /// 한 번의 왕복. 실패하면 종류와(HTTP 오류일 때) 응답 헤더를 돌려준다.
    async fn attempt(
        &self,
        url: &str,
        api_key: &str,
        body: &str,
        attempt: u32,
    ) -> Result<Value, (Failure, Option<HeaderMap>)> {
        let mut request = self
            .http
            .post(url)
            .bearer_auth(api_key)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .timeout(ATTEMPT_TIMEOUT)
            .body(body.to_string());
        if attempt > 0 {
            request = request.header("X-TypeSafe-Retry-Count", attempt.to_string());
        }
        let transport = |error: reqwest::Error| {
            let failure = if error.is_timeout() {
                Failure::Timeout
            } else {
                Failure::Connection
            };
            (failure, None)
        };
        let response = request.send().await.map_err(transport)?;
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.bytes().await.map_err(transport)?;
        if !status.is_success() {
            return Err((
                Failure::Api {
                    status: status.as_u16(),
                },
                Some(headers),
            ));
        }
        Ok(parse_body(&bytes))
    }
}

/// 본문을 JSON으로 읽고, 안 되면 글자 그대로, 비었으면 Null
fn parse_body(bytes: &[u8]) -> Value {
    if bytes.is_empty() {
        return Value::Null;
    }
    serde_json::from_slice(bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(bytes).into_owned()))
}

fn to_decider_error(failure: &Failure) -> DeciderError {
    match failure {
        Failure::Api { status: 401 | 403 } => DeciderError::new(
            "TypeSafe 키가 맞지 않거나 권한이 없습니다.",
            DeciderErrorKind::Unauthorized,
        ),
        Failure::Api { status: 429 } => DeciderError::new(
            "TypeSafe 호출 한도에 걸렸습니다.",
            DeciderErrorKind::RateLimited,
        ),
        Failure::Timeout => DeciderError::new(
            "TypeSafe 응답이 늦어 중단했습니다.",
            DeciderErrorKind::Timeout,
        ),
        Failure::Api { status } => DeciderError::new(
            format!("TypeSafe 오류(HTTP {status})"),
            if (400..500).contains(status) {
                DeciderErrorKind::InvalidRequest
            } else {
                DeciderErrorKind::Upstream
            },
        ),
        Failure::Connection => DeciderError::new(
            "TypeSafe에 연결하지 못했습니다.",
            DeciderErrorKind::Upstream,
        ),
    }
}

fn bad_format() -> DeciderError {
    DeciderError::new("Jev 응답 형식이 다릅니다.", DeciderErrorKind::Upstream)
}

/// `{ score, confidence }` 모양이면 (점수, 확신도)
fn score_answer(answer: Option<&Value>) -> Option<(f64, f64)> {
    let answer = answer?;
    Some((
        answer.get("score")?.as_f64()?,
        answer.get("confidence")?.as_f64()?,
    ))
}

/// 0~5단계 기댓값을 0~10으로
fn to_ten(value: f64) -> f64 {
    (value * RELEVANCE_SCALE).clamp(0.0, 10.0)
}

/// 토큰 수를 알려 주지 않으면(null) 0으로 센다.
fn token_usage(result: &Value) -> TokenUsage {
    let count = |key: &str| {
        result
            .pointer(&format!("/usage/{key}"))
            .and_then(Value::as_f64)
            .map_or(0, |v| v as i64)
    };
    TokenUsage {
        input_tokens: count("input_tokens"),
        output_tokens: count("output_tokens"),
    }
}

/// JS `Number(value ?? 0)`
fn number_or_zero(value: Option<&Value>) -> f64 {
    match value {
        None | Some(Value::Null) => 0.0,
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => js_parse_number(s),
        Some(Value::Bool(b)) => f64::from(u8::from(*b)),
        Some(_) => f64::NAN,
    }
}

impl Decider for TypeSafeDecider {
    async fn decide(&self, input: &DecisionInput, model: &str) -> Result<Decision, DeciderError> {
        let target: JudgeTarget = input.stock.kind;
        let result = self
            .system_one(
                model,
                build_article_state(input),
                build_article_questions(target),
            )
            .await?;
        // 응답을 검사하지 않고 그대로 돌려주므로 저장 전에 모양을 확인한다.
        let relevance = score_answer(result.pointer("/answers/relevance"));
        let magnitude = score_answer(result.pointer("/answers/magnitude"));
        let impact = result.pointer("/answers/impact");
        let choice = impact
            .and_then(|i| i.get("choice"))
            .and_then(Value::as_str)
            .and_then(|c| c.parse::<Impact>().ok());
        let impact_confidence = impact
            .and_then(|i| i.get("confidence"))
            .and_then(Value::as_f64);
        let model_name = result.get("model").and_then(Value::as_str);
        let (
            Some((rel, rel_conf)),
            Some((mag, mag_conf)),
            Some(choice),
            Some(impact_confidence),
            Some(model_name),
        ) = (relevance, magnitude, choice, impact_confidence, model_name)
        else {
            return Err(bad_format());
        };
        // 종목 대상만 bystander를 묻는다. 예/아니오 답은 `noul`(예일 확률)에 있다.
        let bystander = if target == JudgeTarget::Stock {
            let probability = result
                .pointer("/answers/bystander/noul")
                .and_then(Value::as_f64)
                .filter(|p| p.is_finite());
            Some(probability.ok_or_else(bad_format)?)
        } else {
            None
        };
        let probabilities = impact.and_then(|i| i.get("probabilities"));
        let probability = |key: &str| number_or_zero(probabilities.and_then(|p| p.get(key)));
        Ok(Decision {
            relevance: to_ten(rel),
            relevance_confidence: rel_conf,
            magnitude: to_ten(mag),
            magnitude_confidence: mag_conf,
            impact: choice,
            impact_confidence,
            impact_probabilities: [
                probability("positive"),
                probability("negative"),
                probability("neutral"),
            ],
            bystander,
            model: model_name.to_string(),
            usage: token_usage(&result),
        })
    }

    async fn decide_merged(
        &self,
        input: &MergedInput,
        model: &str,
    ) -> Result<MergedDecision, DeciderError> {
        let result = self
            .system_one(
                model,
                build_merged_state(input),
                build_merged_questions(input),
            )
            .await?;
        let scored = |key: &str| {
            score_answer(result.pointer(&format!("/answers/{key}"))).map(|(score, confidence)| {
                Scored {
                    score: to_ten(score),
                    confidence,
                }
            })
        };
        let model_name = result.get("model").and_then(Value::as_str);
        let mut topics = Vec::with_capacity(input.topics.len());
        for n in 1..=input.topics.len() {
            topics.push(scored(&format!("relevance_t{n}")).ok_or_else(bad_format)?);
        }
        let mut markets = Vec::with_capacity(input.markets.len());
        for &market in &input.markets {
            let key = market_key(market);
            let relevance = if input.market_items.contains(&market) {
                Some(scored(&format!("relevance_{key}")).ok_or_else(bad_format)?)
            } else {
                None
            };
            let magnitude = scored(&format!("magnitude_{key}")).ok_or_else(bad_format)?;
            let impact = result.pointer(&format!("/answers/impact_{key}"));
            let choice = impact
                .and_then(|i| i.get("choice"))
                .and_then(Value::as_str)
                .and_then(|c| c.parse::<Impact>().ok())
                .ok_or_else(bad_format)?;
            let impact_confidence = impact
                .and_then(|i| i.get("confidence"))
                .and_then(Value::as_f64)
                .ok_or_else(bad_format)?;
            let probabilities = impact.and_then(|i| i.get("probabilities"));
            let probability = |name: &str| number_or_zero(probabilities.and_then(|p| p.get(name)));
            markets.push(MarketVerdict {
                market,
                relevance,
                magnitude,
                impact: choice,
                impact_confidence,
                impact_probabilities: [
                    probability("positive"),
                    probability("negative"),
                    probability("neutral"),
                ],
            });
        }
        let Some(model_name) = model_name else {
            return Err(bad_format());
        };
        Ok(MergedDecision {
            topics,
            markets,
            model: model_name.to_string(),
            usage: token_usage(&result),
        })
    }

    async fn rate(&self, input: &LeadInput, model: &str) -> Result<LeadRating, DeciderError> {
        let result = self
            .system_one(
                model,
                build_lead_state(input),
                build_lead_questions(input.stock.kind),
            )
            .await?;
        let relevance = score_answer(result.pointer("/answers/relevance"));
        let magnitude = score_answer(result.pointer("/answers/magnitude"));
        let model_name = result.get("model").and_then(Value::as_str);
        let (Some((rel, rel_conf)), Some((mag, mag_conf)), Some(model_name)) =
            (relevance, magnitude, model_name)
        else {
            return Err(bad_format());
        };
        Ok(LeadRating {
            relevance: to_ten(rel),
            relevance_confidence: rel_conf,
            magnitude: to_ten(mag),
            magnitude_confidence: mag_conf,
            model: model_name.to_string(),
            usage: token_usage(&result),
        })
    }
}

#[cfg(test)]
#[path = "typesafe_tests.rs"]
mod tests;
