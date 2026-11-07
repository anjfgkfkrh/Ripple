//! 한국투자증권 KIS Open API로 한국 종목 현재가를 받는다(13단계, 2026-10-30 결정).
//! 주소·필드는 공식 예제(github.com/koreainvestment/open-trading-api, examples_llm/domestic_stock/inquire_price,
//! examples_user/kis_auth.py)를 따른다. 시세 조회만 하고 주문 API는 쓰지 않는다.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Map, Value};

use crate::server::http_client::new_client;
use crate::server::text::{js_parse_number, js_trim};
use crate::server::time::{EpochMs, now_ms};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KisEnv {
    Demo,
    Real,
}

pub const KIS_DEMO_BASE_URL: &str = "https://openapivts.koreainvestment.com:29443";
pub const KIS_REAL_BASE_URL: &str = "https://openapi.koreainvestment.com:9443";

pub fn kis_base_url(env: KisEnv) -> &'static str {
    match env {
        KisEnv::Demo => KIS_DEMO_BASE_URL,
        KisEnv::Real => KIS_REAL_BASE_URL,
    }
}

/// 주식현재가 시세
const PRICE_PATH: &str = "/uapi/domestic-stock/v1/quotations/inquire-price";
const PRICE_TR_ID: &str = "FHKST01010100";
/// 통합(KRX + 넥스트레이드, 2026-10-30 결정). J는 KRX, NX는 넥스트레이드만
pub const KIS_MARKET_CODE: &str = "UN";
/// 토큰은 1분에 1번만 발급된다(공식 예제 README). 실패하면 이만큼 기다린 뒤 다시 받는다.
pub const TOKEN_RETRY_MS: i64 = 61 * 1000;
/// 만료 이만큼 전에 새로 받는다.
const TOKEN_REFRESH_MARGIN_MS: i64 = 10 * 60 * 1000;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq)]
pub struct KisQuote {
    /// 현재가(원)
    pub price: f64,
    /// 기준가(전일 종가) 대비
    pub change: f64,
    /// 기준가 대비 등락률(%)
    pub change_percent: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct KisError(pub String);

/// 종목코드(6자리) 하나의 현재가를 주는 쪽(`ReqwestKisClient`, 테스트의 가짜)
pub trait KisClient: Send + Sync {
    fn domestic_quote(
        &self,
        code: &str,
    ) -> impl std::future::Future<Output = Result<KisQuote, KisError>> + Send;
}

type NowFn = Arc<dyn Fn() -> EpochMs + Send + Sync>;

pub struct KisOptions {
    pub app_key: String,
    pub app_secret: String,
    pub env: KisEnv,
    /// 테스트용 가짜 서버 주소. 없으면 env의 공식 주소
    pub base_url: Option<String>,
    pub now: Option<NowFn>,
    /// 요청 시작 간격(호출 한도 대비). 없으면 1.1초
    pub min_interval_ms: Option<i64>,
    pub timeout: Option<Duration>,
    /// 나눠 쓸 HTTP 클라이언트(R9 결정 ③, 설계 8장). 없으면 새로 만든다.
    pub http: Option<reqwest::Client>,
}

struct State {
    /// 토큰 값과 만료 시각. worker 메모리에만 둔다(다시 시작하면 새로 받음, 2026-10-30 결정).
    token: Option<(String, EpochMs)>,
    /// 마지막으로 토큰 발급에 실패한 시각(없으면 None)
    token_failed_at: Option<EpochMs>,
    next_at: EpochMs,
}

pub struct ReqwestKisClient {
    http: reqwest::Client,
    app_key: String,
    app_secret: String,
    base: String,
    now: NowFn,
    min_interval_ms: i64,
    timeout: Duration,
    state: Mutex<State>,
}

impl ReqwestKisClient {
    pub fn new(options: KisOptions) -> Self {
        let base = options
            .base_url
            .unwrap_or_else(|| kis_base_url(options.env).to_string());
        Self {
            http: options.http.unwrap_or_else(new_client),
            app_key: options.app_key,
            app_secret: options.app_secret,
            base: base.trim_end_matches('/').to_string(),
            now: options.now.unwrap_or_else(|| Arc::new(now_ms)),
            min_interval_ms: options.min_interval_ms.unwrap_or(1100),
            timeout: options.timeout.unwrap_or(Duration::from_secs(20)),
            state: Mutex::new(State {
                token: None,
                token_failed_at: None,
                next_at: 0,
            }),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("KIS 상태 잠금이 깨졌습니다")
    }

    /// 요청을 보내고 (HTTP 상태, JSON 객체 본문)을 돌려준다. 시작 간격을 지킨다.
    async fn request(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> Result<(u16, Map<String, Value>), KisError> {
        let wait = {
            let mut state = self.state();
            let now = (self.now)();
            let wait = state.next_at - now;
            state.next_at = now.max(state.next_at) + self.min_interval_ms;
            wait
        };
        if wait > 0 {
            tokio::time::sleep(Duration::from_millis(wait as u64)).await;
        }
        let response = builder.timeout(self.timeout).send().await.map_err(|e| {
            KisError(if e.is_timeout() {
                "KIS 응답이 늦어 중단했습니다.".to_string()
            } else {
                format!("KIS에 연결하지 못했습니다: {e}")
            })
        })?;
        let status = response.status().as_u16();
        let bytes = response
            .bytes()
            .await
            .map_err(|_| KisError(format!("KIS 응답을 읽지 못했습니다(HTTP {status}).")))?;
        let body: Value = serde_json::from_slice(&bytes)
            .map_err(|_| KisError(format!("KIS 응답을 읽지 못했습니다(HTTP {status}).")))?;
        Ok((
            status,
            match body {
                Value::Object(map) => map,
                _ => Map::new(),
            },
        ))
    }

    async fn access_token(&self) -> Result<String, KisError> {
        let at = (self.now)();
        {
            let state = self.state();
            if let Some((value, expires_at)) = &state.token
                && expires_at - TOKEN_REFRESH_MARGIN_MS > at
            {
                return Ok(value.clone());
            }
            if let Some(failed_at) = state.token_failed_at
                && at - failed_at < TOKEN_RETRY_MS
            {
                return Err(KisError(
                    "KIS 토큰 발급이 방금 실패해 1분 뒤 다시 받습니다.".into(),
                ));
            }
        }
        let builder = self
            .http
            .post(format!("{}/oauth2/tokenP", self.base))
            .header("content-type", "application/json; charset=utf-8")
            .body(
                serde_json::json!({
                    "grant_type": "client_credentials",
                    "appkey": self.app_key,
                    "appsecret": self.app_secret,
                })
                .to_string(),
            );
        let (status, body) = self.request(builder).await?;
        let value = body
            .get("access_token")
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty());
        if status != 200 || value.is_none() {
            self.state().token_failed_at = Some(at);
            let reason = match body.get("error_description").and_then(Value::as_str) {
                Some(text) => text.to_string(),
                None => format!("HTTP {status}"),
            };
            return Err(KisError(format!("KIS 토큰을 받지 못했습니다: {reason}")));
        }
        let value = value.expect("위에서 확인했다").to_string();
        self.state().token = Some((value.clone(), at + token_lifetime_ms(&body)));
        Ok(value)
    }

    async fn quote(&self, code: &str) -> Result<KisQuote, KisError> {
        let mut retried = false;
        loop {
            let query = url::form_urlencoded::Serializer::new(String::new())
                .append_pair("FID_COND_MRKT_DIV_CODE", KIS_MARKET_CODE)
                .append_pair("FID_INPUT_ISCD", code)
                .finish();
            let token = self.access_token().await?;
            let builder = self
                .http
                .get(format!("{}{PRICE_PATH}?{query}", self.base))
                .header("content-type", "application/json; charset=utf-8")
                .header("authorization", format!("Bearer {token}"))
                .header("appkey", &self.app_key)
                .header("appsecret", &self.app_secret)
                .header("tr_id", PRICE_TR_ID)
                .header("custtype", "P");
            let (status, body) = self.request(builder).await?;
            if status == 200 && body.get("rt_cd") == Some(&Value::String("0".into())) {
                return parse_quote(body.get("output"));
            }
            let msg_code = body.get("msg_cd").and_then(Value::as_str).unwrap_or("");
            // EGW001xx는 토큰 문제(만료·잘못된 토큰). 새 토큰으로 한 번만 다시 묻는다.
            if msg_code.starts_with("EGW001") && !retried {
                self.state().token = None;
                retried = true;
                continue;
            }
            let message = body
                .get("msg1")
                .and_then(Value::as_str)
                .map(|m| js_trim(m).to_string())
                .unwrap_or_default();
            let mut details = vec![format!("HTTP {status}")];
            if !msg_code.is_empty() {
                details.push(msg_code.to_string());
            }
            return Err(KisError(format!(
                "KIS 시세 오류({}){}",
                details.join(", "),
                if message.is_empty() {
                    String::new()
                } else {
                    format!(": {message}")
                }
            )));
        }
    }
}

impl KisClient for ReqwestKisClient {
    fn domestic_quote(
        &self,
        code: &str,
    ) -> impl std::future::Future<Output = Result<KisQuote, KisError>> + Send {
        self.quote(code)
    }
}

/// JS `Number(value)`: 글자는 숫자로 읽고, 숫자는 그대로, 그 밖은 NaN으로 본다(없는 값·null·객체는 어차피 거른다).
fn number_of(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => js_parse_number(s),
        _ => f64::NAN,
    }
}

/// 응답의 expires_in(초)을 쓰고, 없으면 하루(공식 예제 주석)로 본다.
fn token_lifetime_ms(body: &Map<String, Value>) -> i64 {
    let seconds = number_of(body.get("expires_in"));
    if seconds.is_finite() && seconds > 0.0 {
        (seconds * 1000.0) as i64
    } else {
        DAY_MS
    }
}

/// 현재가(stck_prpr)와 기준가(stck_sdpr)로 대비·등락률을 계산한다(부호 칸을 해석하지 않아도 되게).
pub fn parse_quote(output: Option<&Value>) -> Result<KisQuote, KisError> {
    let row = output.and_then(Value::as_object);
    let field = |key: &str| number_of(row.and_then(|r| r.get(key)));
    let price = field("stck_prpr");
    let base = field("stck_sdpr");
    // JS `!(x > 0)`: NaN이거나 0 이하
    if price.is_nan() || price <= 0.0 || base.is_nan() || base <= 0.0 {
        return Err(KisError("KIS 시세 응답에 현재가·기준가가 없습니다.".into()));
    }
    let change = price - base;
    Ok(KisQuote {
        price,
        change,
        // JS Math.round(.5는 +∞ 쪽)와 같게 반올림한다.
        change_percent: ((change / base) * 10000.0 + 0.5).floor() / 100.0,
    })
}

#[cfg(test)]
#[path = "kis_tests.rs"]
mod tests;
