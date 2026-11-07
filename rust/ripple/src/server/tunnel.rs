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

/// cloudflared 연결 상태(고정 주소 모드, 15단계). 메트릭 서버의 `/ready`는 연결이 하나 이상이면 200,
/// 아니면 503이고 본문에 `readyConnections`를 준다(cloudflared 2026.10.0 `metrics/readiness.go`로 확인).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadyStatus {
    /// 메트릭 주소가 없어 알 수 없음
    Unknown,
    Connected {
        connections: Option<u64>,
    },
    Disconnected,
    Error {
        reason: String,
    },
}

/// `PUBLIC_URL`을 `https://호스트` 꼴로 검사한다. 끝 `/` 하나는 떼어 낸다. 틀리면 `None`.
pub fn parse_public_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let rest = raw.strip_prefix("https://")?;
    let host = rest.strip_suffix('/').unwrap_or(rest);
    HOSTNAME.is_match(host).then(|| format!("https://{host}"))
}

pub async fn fetch_ready_status(
    metrics_url: Option<&str>,
    http: &reqwest::Client,
    timeout_ms: u64,
) -> ReadyStatus {
    let Some(metrics_url) = metrics_url.map(str::trim).filter(|u| !u.is_empty()) else {
        return ReadyStatus::Unknown;
    };
    let url = match url::Url::parse(metrics_url).and_then(|base| base.join("/ready")) {
        Ok(url) => url,
        Err(error) => {
            return ReadyStatus::Error {
                reason: error.to_string(),
            };
        }
    };
    let response = match http
        .get(url)
        .timeout(Duration::from_millis(timeout_ms))
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            return ReadyStatus::Error {
                reason: describe_request_error(&error),
            };
        }
    };
    if response.status().as_u16() != 200 {
        return ReadyStatus::Disconnected;
    }
    let connections = response
        .json::<Value>()
        .await
        .ok()
        .and_then(|body| body.get("readyConnections").and_then(Value::as_u64));
    ReadyStatus::Connected { connections }
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
