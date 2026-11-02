use chrono::DateTime;
use serde_json::Value;

use crate::server::collect::http::HttpClient;
use crate::server::text::encode_uri_component;
use crate::server::time::EpochMs;

pub const FINNHUB_BASE_URL: &str = "https://finnhub.io/api/v1";

/// Finnhub 뉴스 한 건(공식 응답 필드 중 쓰는 것만)
#[derive(Debug, Clone, PartialEq)]
pub struct FinnhubNews {
    pub id: Option<f64>,
    pub headline: String,
    pub summary: Option<String>,
    /// Finnhub를 거치는 주소(누르면 원문으로 이동)
    pub url: String,
    pub source: Option<String>,
    /// 초 단위 유닉스 시각
    pub datetime: f64,
}

/// 미국 종목 현재가(`/quote`, 13단계). 무료 키로 받을 수 있다(2026-10-30 확인, 한국 종목은 403).
#[derive(Debug, Clone, PartialEq)]
pub struct FinnhubQuote {
    /// 현재가
    pub price: f64,
    /// 전일 종가 대비
    pub change: f64,
    /// 전일 대비 등락률(%)
    pub change_percent: f64,
    /// 시세 시각
    pub quoted_at: EpochMs,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct FinnhubError(pub String);

/// 뉴스를 주는 쪽(`FinnhubClient`, 테스트의 가짜)
pub trait FinnhubNewsClient: Send + Sync {
    fn company_news(
        &self,
        symbol: &str,
        from: EpochMs,
        to: EpochMs,
    ) -> impl std::future::Future<Output = Result<Vec<FinnhubNews>, FinnhubError>> + Send;

    fn general_news(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<FinnhubNews>, FinnhubError>> + Send;
}

impl<H: HttpClient> FinnhubNewsClient for FinnhubClient<H> {
    fn company_news(
        &self,
        symbol: &str,
        from: EpochMs,
        to: EpochMs,
    ) -> impl std::future::Future<Output = Result<Vec<FinnhubNews>, FinnhubError>> + Send {
        FinnhubClient::company_news(self, symbol, from, to)
    }

    fn general_news(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<FinnhubNews>, FinnhubError>> + Send {
        FinnhubClient::general_news(self)
    }
}

/// 하나를 수집과 주가 받기가 나눠 쓸 때
impl<T: FinnhubNewsClient> FinnhubNewsClient for std::sync::Arc<T> {
    fn company_news(
        &self,
        symbol: &str,
        from: EpochMs,
        to: EpochMs,
    ) -> impl std::future::Future<Output = Result<Vec<FinnhubNews>, FinnhubError>> + Send {
        (**self).company_news(symbol, from, to)
    }

    fn general_news(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<FinnhubNews>, FinnhubError>> + Send {
        (**self).general_news()
    }
}

/// 현재가를 주는 쪽(`FinnhubClient`, 테스트의 가짜). 값이 없으면 None
pub trait FinnhubQuoteClient: Send + Sync {
    fn quote(
        &self,
        symbol: &str,
    ) -> impl std::future::Future<Output = Result<Option<FinnhubQuote>, FinnhubError>> + Send;
}

impl<T: FinnhubQuoteClient> FinnhubQuoteClient for std::sync::Arc<T> {
    fn quote(
        &self,
        symbol: &str,
    ) -> impl std::future::Future<Output = Result<Option<FinnhubQuote>, FinnhubError>> + Send {
        (**self).quote(symbol)
    }
}

impl<H: HttpClient> FinnhubQuoteClient for FinnhubClient<H> {
    fn quote(
        &self,
        symbol: &str,
    ) -> impl std::future::Future<Output = Result<Option<FinnhubQuote>, FinnhubError>> + Send {
        FinnhubClient::quote(self, symbol)
    }
}

pub struct FinnhubClient<H> {
    http: H,
    api_key: String,
    base: String,
}

impl<H: HttpClient> FinnhubClient<H> {
    pub fn new(http: H, api_key: String, base_url: Option<String>) -> Self {
        let base = base_url.unwrap_or_else(|| FINNHUB_BASE_URL.to_string());
        Self {
            http,
            api_key,
            base: base.trim_end_matches('/').to_string(),
        }
    }

    async fn get(&self, path: &str) -> Result<Value, FinnhubError> {
        let response = self
            .http
            .get(
                &format!("{}{path}", self.base),
                &[("X-Finnhub-Token", &self.api_key)],
            )
            .await
            .map_err(|e| FinnhubError(e.message))?;
        match response.status {
            401 | 403 => {
                return Err(FinnhubError(
                    "Finnhub 키가 맞지 않거나 권한이 없습니다.".into(),
                ));
            }
            429 => return Err(FinnhubError("Finnhub 호출 한도를 넘었습니다.".into())),
            200 => {}
            status => return Err(FinnhubError(format!("Finnhub 응답 오류(HTTP {status})"))),
        }
        serde_json::from_str(&response.text)
            .map_err(|_| FinnhubError("Finnhub 응답을 읽지 못했습니다.".into()))
    }

    async fn news(&self, path: &str) -> Result<Vec<FinnhubNews>, FinnhubError> {
        let body = self.get(path).await?;
        let Value::Array(items) = body else {
            return Err(FinnhubError("Finnhub 응답 형식이 다릅니다.".into()));
        };
        Ok(items
            .iter()
            .filter_map(|n| {
                Some(FinnhubNews {
                    id: n.get("id").and_then(Value::as_f64),
                    headline: n.get("headline")?.as_str()?.to_string(),
                    summary: n.get("summary").and_then(Value::as_str).map(str::to_string),
                    url: n.get("url")?.as_str()?.to_string(),
                    source: n.get("source").and_then(Value::as_str).map(str::to_string),
                    datetime: n.get("datetime")?.as_f64()?,
                })
            })
            .collect())
    }

    /// 미국 종목 하나의 뉴스(종목에 바로 연결한다)
    pub async fn company_news(
        &self,
        symbol: &str,
        from: EpochMs,
        to: EpochMs,
    ) -> Result<Vec<FinnhubNews>, FinnhubError> {
        self.news(&format!(
            "/company-news?symbol={}&from={}&to={}",
            encode_uri_component(symbol),
            day(from),
            day(to)
        ))
        .await
    }

    /// 시장 전체 뉴스(영어 키워드로 종목에 건다)
    pub async fn general_news(&self) -> Result<Vec<FinnhubNews>, FinnhubError> {
        self.news("/news?category=general").await
    }

    /// 값이 없으면(현재가 0) None. 무료 키로 못 받는 티커(모르는 티커·한국 종목)는 403 오류(2026-10-30 확인)
    pub async fn quote(&self, symbol: &str) -> Result<Option<FinnhubQuote>, FinnhubError> {
        let body = self
            .get(&format!("/quote?symbol={}", encode_uri_component(symbol)))
            .await?;
        let field = |key: &str| body.as_object().and_then(|o| o.get(key));
        let (c, d, dp, t) = (field("c"), field("d"), field("dp"), field("t"));
        // 숫자이거나 null이어야 한다(없는 칸은 undefined라 통과하지 못한다)
        let numeric_or_null = |v: Option<&Value>| matches!(v, Some(Value::Number(_) | Value::Null));
        if ![c, d, dp, t].into_iter().all(numeric_or_null) {
            return Err(FinnhubError("Finnhub 응답 형식이 다릅니다.".into()));
        }
        let (Some(price), Some(time)) = (c.and_then(Value::as_f64), t.and_then(Value::as_f64))
        else {
            return Ok(None);
        };
        if price <= 0.0 {
            return Ok(None);
        }
        Ok(Some(FinnhubQuote {
            price,
            change: d.and_then(Value::as_f64).unwrap_or(0.0),
            change_percent: dp.and_then(Value::as_f64).unwrap_or(0.0),
            quoted_at: (time * 1000.0) as i64,
        }))
    }
}

/// Finnhub는 날짜(YYYY-MM-DD, UTC)로 기간을 받는다.
fn day(ms: EpochMs) -> String {
    DateTime::from_timestamp_millis(ms)
        .expect("시각이 달력 범위를 벗어났습니다")
        .format("%Y-%m-%d")
        .to_string()
}

#[cfg(test)]
#[path = "finnhub_tests.rs"]
mod tests;
