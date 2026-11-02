use sha2::{Digest, Sha256};

use crate::server::auth::config::MIN_SECRET_LENGTH;
use crate::server::env::{ConfigError, Env};
use crate::server::text::js_len;

/// web → worker 내부 API 공유 토큰(SPEC 8장). web·worker 둘 다 같은 값을 쓴다.
/// 없거나 짧으면 시작하지 않는다. 값은 오류 메시지에 넣지 않는다.
pub fn read_worker_api_token(env: &Env) -> Result<String, ConfigError> {
    let token = env.get("WORKER_API_TOKEN").unwrap_or("");
    if js_len(token) < MIN_SECRET_LENGTH {
        return Err(ConfigError(format!(
            "WORKER_API_TOKEN은 {MIN_SECRET_LENGTH}자 이상이어야 합니다."
        )));
    }
    Ok(token.to_string())
}

/// "Bearer <토큰>" 헤더가 맞는지 같은 시간에 비교한다(둘 다 SHA-256으로 길이를 맞춘 뒤 모든 바이트를 본다).
pub fn is_authorized_bearer(header: Option<&str>, token: &str) -> bool {
    let presented = header
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or("");
    let a = Sha256::digest(presented.as_bytes());
    let b = Sha256::digest(token.as_bytes());
    let same = a
        .iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0;
    !presented.is_empty() && same
}

#[cfg(test)]
#[path = "worker_token_tests.rs"]
mod tests;
