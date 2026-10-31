//! 수집용 HTTP 클라이언트. 같은 사이트에는 요청 시작 간격을 둔다(CLAUDE.md: 크롤링은 요청 간격을 둔다).
//! User-Agent는 개인용 RSS 리더임을 그대로 밝힌다.
use std::collections::HashMap;
use std::future::Future;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use regex::Regex;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use tokio::time::Instant;

pub const COLLECTOR_USER_AGENT: &str = "Mozilla/5.0 (compatible; Ripple/0.1; personal RSS reader)";

const ACCEPT: &str = "application/rss+xml, application/atom+xml, application/xml;q=0.9, application/json;q=0.9, */*;q=0.5";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub text: String,
    pub content_type: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpErrorKind {
    Timeout,
    Network,
    TooLarge,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct HttpError {
    pub message: String,
    pub kind: HttpErrorKind,
}

impl HttpError {
    pub fn new(message: impl Into<String>, kind: HttpErrorKind) -> Self {
        Self {
            message: message.into(),
            kind,
        }
    }
}

/// 주소 하나를 받아 오는 쪽(수집용 클라이언트, 테스트의 가짜). `headers`는 기본 헤더를 덮어쓰거나 더한다.
pub trait HttpClient: Send + Sync {
    fn get(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> impl Future<Output = Result<HttpResponse, HttpError>> + Send;
}

pub struct HttpClientOptions {
    pub user_agent: String,
    /// 같은 호스트에 대한 요청 시작 간격
    pub min_interval_ms: u64,
    pub timeout_ms: u64,
    pub max_bytes: usize,
}

impl Default for HttpClientOptions {
    fn default() -> Self {
        Self {
            user_agent: COLLECTOR_USER_AGENT.to_string(),
            min_interval_ms: 1100,
            timeout_ms: 20_000,
            max_bytes: 5_000_000,
        }
    }
}

/// `reqwest`로 받는 수집용 클라이언트
pub struct ReqwestHttpClient {
    http: reqwest::Client,
    options: HttpClientOptions,
    /// 호스트마다 다음 요청을 시작해도 되는 시각. 자리를 먼저 잡아 두므로 동시에 불러도 간격이 지켜진다.
    next_at: Mutex<HashMap<String, Instant>>,
}

impl ReqwestHttpClient {
    pub fn new(http: reqwest::Client, options: HttpClientOptions) -> Self {
        Self {
            http,
            options,
            next_at: Mutex::new(HashMap::new()),
        }
    }

    async fn wait_turn(&self, host: &str) {
        let now = Instant::now();
        let at = {
            let mut next_at = self.next_at.lock().expect("호스트 간격 잠금이 깨졌습니다");
            let at = next_at.get(host).map_or(now, |t| (*t).max(now));
            next_at.insert(
                host.to_string(),
                at + Duration::from_millis(self.options.min_interval_ms),
            );
            at
        };
        if at > now {
            tokio::time::sleep_until(at).await;
        }
    }
}

/// JS `new URL(url).host`(포트가 기본값이 아니면 `:포트`까지)
fn host_of(url: &url::Url) -> String {
    let host = url.host_str().unwrap_or("");
    match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    }
}

impl HttpClient for ReqwestHttpClient {
    async fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<HttpResponse, HttpError> {
        let parsed = url::Url::parse(url).map_err(|e| {
            HttpError::new(format!("연결하지 못했습니다: {e}"), HttpErrorKind::Network)
        })?;
        self.wait_turn(&host_of(&parsed)).await;

        let mut map = HeaderMap::new();
        let invalid = |what: &str| {
            HttpError::new(
                format!("연결하지 못했습니다: 올바르지 않은 {what}"),
                HttpErrorKind::Network,
            )
        };
        map.insert(
            "user-agent",
            HeaderValue::from_str(&self.options.user_agent).map_err(|_| invalid("User-Agent"))?,
        );
        map.insert("accept", HeaderValue::from_static(ACCEPT));
        for (name, value) in headers {
            let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| invalid("헤더 이름"))?;
            map.insert(
                name,
                HeaderValue::from_str(value).map_err(|_| invalid("헤더 값"))?,
            );
        }
        let started = self
            .http
            .get(url)
            .headers(map)
            .timeout(Duration::from_millis(self.options.timeout_ms))
            .send()
            .await;
        let mut response = started.map_err(|e| to_http_error(&e))?;
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        // 받으면서 크기를 센다. 한도를 넘으면 더 받지 않고 끊는다.
        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| to_http_error(&e))? {
            if bytes.len() + chunk.len() > self.options.max_bytes {
                return Err(HttpError::new(
                    format!("응답이 너무 큽니다({}바이트 초과).", self.options.max_bytes),
                    HttpErrorKind::TooLarge,
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(HttpResponse {
            status,
            text: decode(&bytes, &content_type),
            content_type,
        })
    }
}

fn to_http_error(error: &reqwest::Error) -> HttpError {
    if error.is_timeout() {
        HttpError::new("응답이 늦어 중단했습니다.", HttpErrorKind::Timeout)
    } else {
        HttpError::new(
            format!("연결하지 못했습니다: {error}"),
            HttpErrorKind::Network,
        )
    }
}

static CHARSET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)charset=["']?([A-Za-z0-9_-]+)"#).expect("정규식이 올바르다")
});
static XML_ENCODING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<\?xml[^>]*encoding=["']([A-Za-z0-9_-]+)["']"#).expect("정규식이 올바르다")
});

/// Content-Type이나 XML 선언의 인코딩으로 읽는다(EUC-KR 피드 대비). 모르면 UTF-8.
pub fn decode(bytes: &[u8], content_type: &str) -> String {
    // TextDecoder("latin1")은 windows-1252다.
    let (head, _) =
        encoding_rs::WINDOWS_1252.decode_without_bom_handling(&bytes[..bytes.len().min(300)]);
    let label = CHARSET
        .captures(content_type)
        .or_else(|| XML_ENCODING.captures(&head))
        .map_or("utf-8".to_string(), |caps| caps[1].to_string());
    let normalized = match label.to_lowercase().as_str() {
        "cp949" | "ms949" | "x-windows-949" => "euc-kr".to_string(),
        _ => label,
    };
    // `new TextDecoder(label)`처럼 WHATWG 이름으로 찾고, 모르면(또는 replacement면) UTF-8
    let encoding = encoding_rs::Encoding::for_label(normalized.as_bytes())
        .filter(|e| *e != encoding_rs::REPLACEMENT)
        .unwrap_or(encoding_rs::UTF_8);
    encoding.decode_with_bom_removal(bytes).0.into_owned()
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
