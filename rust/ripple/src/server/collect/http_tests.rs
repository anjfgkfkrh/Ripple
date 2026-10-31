use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

fn client(min_interval_ms: u64) -> ReqwestHttpClient {
    client_with(HttpClientOptions {
        min_interval_ms,
        ..HttpClientOptions::default()
    })
}

fn client_with(options: HttpClientOptions) -> ReqwestHttpClient {
    ReqwestHttpClient::new(reqwest::Client::new(), options)
}

async fn ok_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;
    server
}

// describe("createHttpClient")
#[tokio::test(flavor = "current_thread")]
async fn spaces_requests_to_same_site_but_not_to_other_sites() {
    // it("같은 사이트에는 요청 시작 간격을 두고, 다른 사이트는 기다리지 않는다")
    // 호스트는 포트까지 포함하므로 서버 둘은 서로 다른 사이트다.
    let times: Arc<Mutex<Vec<(String, Instant)>>> = Arc::new(Mutex::new(Vec::new()));
    let record = |label: &'static str, times: Arc<Mutex<Vec<(String, Instant)>>>| {
        move |_: &wiremock::Request| {
            times
                .lock()
                .unwrap()
                .push((label.to_string(), Instant::now()));
            ResponseTemplate::new(200).set_body_string("ok")
        }
    };
    let a = MockServer::start().await;
    let b = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(record("a", times.clone()))
        .mount(&a)
        .await;
    Mock::given(method("GET"))
        .respond_with(record("b", times.clone()))
        .mount(&b)
        .await;
    let http = client(80);
    let start = Instant::now();
    let (url_a1, url_a2, url_b1) = (
        format!("{}/1", a.uri()),
        format!("{}/2", a.uri()),
        format!("{}/1", b.uri()),
    );
    let (r1, r2, r3) = tokio::join!(
        http.get(&url_a1, &[]),
        http.get(&url_a2, &[]),
        http.get(&url_b1, &[])
    );
    r1.unwrap();
    r2.unwrap();
    r3.unwrap();
    let times = times.lock().unwrap();
    let at = |label: &str| -> Vec<Duration> {
        times
            .iter()
            .filter(|(l, _)| l == label)
            .map(|(_, t)| *t - start)
            .collect()
    };
    let (a_times, b_times) = (at("a"), at("b"));
    assert!(
        a_times[1] - a_times[0] >= Duration::from_millis(75),
        "{a_times:?}"
    );
    assert!(b_times[0] < Duration::from_millis(40), "{b_times:?}");
}

#[tokio::test(flavor = "current_thread")]
async fn sends_user_agent_naming_ripple_and_extra_headers() {
    // it("개인용 RSS 리더임을 밝히는 User-Agent를 보내고, 헤더를 더할 수 있다")
    let server = ok_server().await;
    client(0)
        .get(&format!("{}/x", server.uri()), &[("X-Token", "t")])
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    let headers = &requests[0].headers;
    assert_eq!(
        headers.get("user-agent").unwrap().to_str().unwrap(),
        COLLECTOR_USER_AGENT
    );
    assert_eq!(headers.get("x-token").unwrap().to_str().unwrap(), "t");
    assert!(
        headers
            .get("accept")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("application/rss+xml")
    );
    assert!(COLLECTOR_USER_AGENT.contains("Ripple"));
}

#[tokio::test(flavor = "current_thread")]
async fn returns_status_and_content_type() {
    // it("상태 코드와 Content-Type을 돌려준다")
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/x"))
        .respond_with(ResponseTemplate::new(404).set_body_raw("missing", "text/plain"))
        .mount(&server)
        .await;
    let response = client(0)
        .get(&format!("{}/x", server.uri()), &[])
        .await
        .unwrap();
    assert_eq!(
        response,
        HttpResponse {
            status: 404,
            text: "missing".into(),
            content_type: "text/plain".into()
        }
    );
}

#[tokio::test(flavor = "current_thread")]
async fn too_large_failed_and_slow_are_http_errors() {
    // it("너무 큰 응답·연결 실패·시간 초과는 HttpError")
    let big = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(100)))
        .mount(&big)
        .await;
    let limited = client_with(HttpClientOptions {
        min_interval_ms: 0,
        max_bytes: 10,
        ..HttpClientOptions::default()
    });
    let error = limited
        .get(&format!("{}/big", big.uri()), &[])
        .await
        .unwrap_err();
    assert_eq!(error.kind, HttpErrorKind::TooLarge);
    assert!(error.message.contains("10바이트"));

    let down = client(0)
        .get("http://127.0.0.1:1/x", &[])
        .await
        .unwrap_err();
    assert_eq!(down.kind, HttpErrorKind::Network);
    assert!(down.message.starts_with("연결하지 못했습니다: "));

    let slow_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("late")
                .set_delay(Duration::from_millis(500)),
        )
        .mount(&slow_server)
        .await;
    let slow = client_with(HttpClientOptions {
        min_interval_ms: 0,
        timeout_ms: 30,
        ..HttpClientOptions::default()
    });
    let error = slow
        .get(&format!("{}/slow", slow_server.uri()), &[])
        .await
        .unwrap_err();
    assert_eq!(error.kind, HttpErrorKind::Timeout);
    assert_eq!(error.message, "응답이 늦어 중단했습니다.");
}

#[tokio::test(flavor = "current_thread")]
async fn invalid_url_is_a_network_error_not_a_panic() {
    // Rust에서만 생기는 경우: 주소를 읽지 못해도 오류로 돌려준다(JS는 `new URL`이 예외를 던진다).
    let error = client(0).get("not a url", &[]).await.unwrap_err();
    assert_eq!(error.kind, HttpErrorKind::Network);
}

#[tokio::test(flavor = "current_thread")]
async fn redirects_are_followed() {
    // `redirect: "follow"`
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/old"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/new"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/new"))
        .respond_with(ResponseTemplate::new(200).set_body_string("moved"))
        .mount(&server)
        .await;
    let response = client(0)
        .get(&format!("{}/old", server.uri()), &[])
        .await
        .unwrap();
    assert_eq!((response.status, response.text.as_str()), (200, "moved"));
}

// describe("decode")
#[test]
fn follows_content_type_or_xml_declaration_encoding() {
    // it("Content-Type이나 XML 선언의 인코딩을 따른다(EUC-KR 피드)")
    // "삼성" (EUC-KR)
    let euc_kr: [u8; 4] = [0xbb, 0xef, 0xbc, 0xba];
    assert_eq!(decode(&euc_kr, "text/xml; charset=EUC-KR"), "삼성");
    let mut declared = br#"<?xml version="1.0" encoding="euc-kr"?>"#.to_vec();
    declared.extend_from_slice(&euc_kr);
    assert!(decode(&declared, "text/xml").contains("삼성"));
    assert_eq!(decode(&euc_kr, "text/xml; charset=cp949"), "삼성");
}

#[test]
fn unknown_encoding_falls_back_to_utf8() {
    // it("모르는 인코딩이면 UTF-8로 읽는다")
    assert_eq!(decode("가".as_bytes(), "text/xml; charset=none-such"), "가");
}

#[test]
fn utf8_bom_is_removed_and_default_is_utf8() {
    // Rust에서만 생기는 경우: TextDecoder처럼 UTF-8 BOM은 떼고, 선언이 없으면 UTF-8
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice("가나".as_bytes());
    assert_eq!(decode(&bytes, ""), "가나");
    assert_eq!(decode(&bytes, "text/xml; charset=\"utf-8\""), "가나");
}
