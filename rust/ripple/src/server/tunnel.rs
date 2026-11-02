//! 외부 접속 임시 주소 상태(9단계, `tunnel.ts`).
use std::time::Duration;

use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

/// off: 터널을 쓰지 않음(로컬 실행), pending: 주소를 받는 중
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TunnelStatus {
    Off,
    Pending,
    Ready { url: String },
    Error { reason: String },
}

static HOSTNAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+$").expect("정규식이 올바르다"));

pub const TUNNEL_TIMEOUT_MS: u64 = 1500;

/// cloudflared Quick Tunnel의 지금 주소를 읽는다. cloudflared 메트릭 서버의 `/quicktunnel`은
/// `{"hostname":"…"}`를 준다(2026-10-21 cloudflared 소스로 확인). 주소는 재시작마다 바뀐다.
pub async fn fetch_tunnel_status(
    metrics_url: Option<&str>,
    http: &reqwest::Client,
    timeout_ms: u64,
) -> TunnelStatus {
    let Some(metrics_url) = metrics_url.map(str::trim).filter(|u| !u.is_empty()) else {
        return TunnelStatus::Off;
    };
    match request_hostname(metrics_url, http, timeout_ms).await {
        Ok(status) => status,
        Err(reason) => TunnelStatus::Error { reason },
    }
}

async fn request_hostname(
    metrics_url: &str,
    http: &reqwest::Client,
    timeout_ms: u64,
) -> Result<TunnelStatus, String> {
    let url = url::Url::parse(metrics_url)
        .and_then(|base| base.join("/quicktunnel"))
        .map_err(|error| error.to_string())?;
    let response = http
        .get(url)
        .timeout(Duration::from_millis(timeout_ms))
        .send()
        .await
        .map_err(|error| describe_request_error(&error))?;
    if !response.status().is_success() {
        return Ok(TunnelStatus::Error {
            reason: format!("HTTP {}", response.status().as_u16()),
        });
    }
    let body: Value = response
        .json()
        .await
        .map_err(|error| describe_request_error(&error))?;
    let hostname = body
        .get("hostname")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if hostname.is_empty() {
        return Ok(TunnelStatus::Pending);
    }
    if !HOSTNAME.is_match(hostname) {
        return Ok(TunnelStatus::Error {
            reason: "주소 형식이 다릅니다.".to_string(),
        });
    }
    Ok(TunnelStatus::Ready {
        url: format!("https://{hostname}"),
    })
}

/// `메시지 (원인)` 모양으로 줄인다(TS `describeError`와 같은 꼴, 글자는 reqwest 쪽 문구).
pub fn describe_request_error(error: &reqwest::Error) -> String {
    use std::error::Error;
    let mut deepest: Option<&dyn Error> = None;
    let mut source = error.source();
    while let Some(next) = source {
        deepest = Some(next);
        source = next.source();
    }
    let top = error.to_string();
    match deepest {
        Some(cause) => format!("{top} ({cause})"),
        None => top,
    }
}

#[cfg(test)]
#[path = "tunnel_tests.rs"]
mod tests;
