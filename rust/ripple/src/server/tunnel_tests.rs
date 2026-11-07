use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

async fn status_of(server: &MockServer, response: ResponseTemplate) -> TunnelStatus {
    Mock::given(method("GET"))
        .and(path("/quicktunnel"))
        .respond_with(response)
        .mount(server)
        .await;
    fetch_tunnel_status(Some(&server.uri()), &reqwest::Client::new(), 1500).await
}

// it("터널 주소를 정하지 않았으면 off(로컬 실행)")
#[tokio::test]
async fn off_without_metrics_url() {
    let http = reqwest::Client::new();
    assert_eq!(
        fetch_tunnel_status(None, &http, 1500).await,
        TunnelStatus::Off
    );
    assert_eq!(
        fetch_tunnel_status(Some("  "), &http, 1500).await,
        TunnelStatus::Off
    );
}

// it("/quicktunnel의 hostname으로 https 주소를 만든다. 비어 있으면 받는 중")
#[tokio::test]
async fn builds_https_url_from_hostname() {
    let server = MockServer::start().await;
    let ready = status_of(
        &server,
        ResponseTemplate::new(200)
            .set_body_json(json!({"hostname": "word-word-word.trycloudflare.com"})),
    )
    .await;
    assert_eq!(
        ready,
        TunnelStatus::Ready {
            url: "https://word-word-word.trycloudflare.com".to_string()
        }
    );

    let empty = MockServer::start().await;
    let pending = status_of(
        &empty,
        ResponseTemplate::new(200).set_body_json(json!({"hostname": ""})),
    )
    .await;
    assert_eq!(pending, TunnelStatus::Pending);
}

// it("닿지 못하거나 응답이 이상하면 error와 이유")
#[tokio::test]
async fn error_with_reason() {
    let bad_status = MockServer::start().await;
    assert_eq!(
        status_of(
            &bad_status,
            ResponseTemplate::new(503).set_body_json(json!({}))
        )
        .await,
        TunnelStatus::Error {
            reason: "HTTP 503".to_string()
        }
    );
    let bad_host = MockServer::start().await;
    assert_eq!(
        status_of(
            &bad_host,
            ResponseTemplate::new(200).set_body_json(json!({"hostname": "<script>"}))
        )
        .await,
        TunnelStatus::Error {
            reason: "주소 형식이 다릅니다.".to_string()
        }
    );
}

// Rust 전용: 아무것도 듣지 않는 주소는 error(원인 포함)
#[tokio::test]
async fn unreachable_is_error() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let status = fetch_tunnel_status(
        Some(&format!("http://127.0.0.1:{port}")),
        &reqwest::Client::new(),
        1500,
    )
    .await;
    let TunnelStatus::Error { reason } = status else {
        panic!("error여야 한다");
    };
    assert!(!reason.is_empty());
}

async fn ready_of(server: &MockServer, response: ResponseTemplate) -> ReadyStatus {
    Mock::given(method("GET"))
        .and(path("/ready"))
        .respond_with(response)
        .mount(server)
        .await;
    fetch_ready_status(Some(&server.uri()), &reqwest::Client::new(), 1500).await
}

#[tokio::test]
async fn ready_reads_connection_count() {
    let server = MockServer::start().await;
    let status = ready_of(
        &server,
        ResponseTemplate::new(200).set_body_json(json!({"status": 200, "readyConnections": 4})),
    )
    .await;
    assert_eq!(
        status,
        ReadyStatus::Connected {
            connections: Some(4)
        }
    );

    let no_body = MockServer::start().await;
    assert_eq!(
        ready_of(&no_body, ResponseTemplate::new(200).set_body_string("ok")).await,
        ReadyStatus::Connected { connections: None }
    );
}

#[tokio::test]
async fn ready_503_is_disconnected() {
    let server = MockServer::start().await;
    let status = ready_of(
        &server,
        ResponseTemplate::new(503).set_body_json(json!({"status": 503, "readyConnections": 0})),
    )
    .await;
    assert_eq!(status, ReadyStatus::Disconnected);
}

#[tokio::test]
async fn ready_unreachable_is_error_and_no_url_is_unknown() {
    let http = reqwest::Client::new();
    let status = fetch_ready_status(Some("http://127.0.0.1:1"), &http, 1500).await;
    assert!(matches!(status, ReadyStatus::Error { .. }));
    assert_eq!(
        fetch_ready_status(None, &http, 1500).await,
        ReadyStatus::Unknown
    );
}

#[test]
fn public_url_format() {
    assert_eq!(
        parse_public_url("https://example.com").as_deref(),
        Some("https://example.com")
    );
    assert_eq!(
        parse_public_url(" https://example.com/ ").as_deref(),
        Some("https://example.com")
    );
    assert_eq!(parse_public_url("http://example.com"), None);
    assert_eq!(parse_public_url("https://example.com/path"), None);
    assert_eq!(parse_public_url("https://example.com:8080"), None);
    assert_eq!(parse_public_url(""), None);
}
