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
