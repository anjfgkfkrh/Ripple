use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;

use super::*;

/// 주소 하나를 연 가짜 서버(상태 코드를 정한다)
fn serve(status: StatusCode) -> String {
    let std_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    std_listener.set_nonblocking(true).unwrap();
    let address = std_listener.local_addr().unwrap();
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
                let app = Router::new().route("/health", get(move || async move { status }));
                axum::serve(listener, app).await.unwrap();
            });
    });
    format!("http://{address}/health")
}

#[test]
fn exits_0_for_2xx() {
    assert_eq!(run(&serve(StatusCode::OK)), 0);
}

#[test]
fn exits_1_for_non_2xx() {
    assert_eq!(run(&serve(StatusCode::SERVICE_UNAVAILABLE)), 1);
}

#[test]
fn exits_1_when_connection_fails() {
    // 열려 있지 않은 포트: 잠깐 열었다 닫아 빈 포트를 얻는다.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    assert_eq!(run(&format!("http://127.0.0.1:{port}/health")), 1);
}
