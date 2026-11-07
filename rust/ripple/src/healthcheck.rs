//! `ripple healthcheck <주소>`: 주소를 GET해서 2xx면 종료 코드 0, 아니면 1(compose의 healthcheck가 쓴다).
//! Node가 없는 이미지에서 `node -e fetch(...)`를 대신한다.
use std::time::Duration;

/// compose healthcheck의 제한 시간(5초)보다 짧게 둔다.
const TIMEOUT: Duration = Duration::from_secs(4);

pub fn run(url: &str) -> i32 {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return 1;
    };
    runtime.block_on(async {
        let client = match crate::server::http_client::client_builder()
            .timeout(TIMEOUT)
            .build()
        {
            Ok(client) => client,
            Err(_) => return 1,
        };
        match client.get(url).send().await {
            Ok(response) if response.status().is_success() => 0,
            _ => 1,
        }
    })
}

#[cfg(test)]
#[path = "healthcheck_tests.rs"]
mod tests;
