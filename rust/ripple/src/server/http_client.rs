//! reqwest 클라이언트 만들기. 수집만 OpenSSL(native-tls), 나머지는 rustls.
//! native-tls 기능을 켜면 reqwest 기본값이 native-tls가 되므로 rustls를 직접 고른다.

/// 수집 밖(OpenRouter·Jev·KIS·web 등)에서 쓰는 rustls 클라이언트 설정
pub fn client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder().tls_backend_rustls()
}

pub fn new_client() -> reqwest::Client {
    client_builder()
        .build()
        .expect("HTTP 클라이언트를 만들지 못했습니다")
}

/// 수집(RSS·robots.txt·Finnhub 뉴스)용. 노컷뉴스처럼 TLS 1.2 CBC 암호만 받는 서버가 있는데
/// rustls는 CBC를 지원하지 않아 연결하지 못한다(TS 때 Node가 쓰던 OpenSSL과 같게 맞춤).
pub fn collector_client() -> reqwest::Client {
    reqwest::Client::builder()
        .tls_backend_native()
        .build()
        .expect("수집용 HTTP 클라이언트를 만들지 못했습니다")
}
