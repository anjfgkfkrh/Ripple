//! 로그인 설정(`auth/config.ts`): 비밀번호·세션 서명 키, 세션에 넣는 비밀번호 지문.
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};

use crate::server::env::{ConfigError, Env};
use crate::server::text::js_len;

/// 비밀 값 최소 길이. 이보다 짧으면 web이 시작하지 않는다. (SPEC 8장)
pub const MIN_PASSWORD_LENGTH: usize = 7;
pub const MIN_SECRET_LENGTH: usize = 32;

#[derive(Clone)]
pub struct AuthConfig {
    pub password: String,
    /// 세션 서명 키(HMAC-SHA256)
    pub secret_key: Vec<u8>,
    /// 세션에 넣는 비밀번호 지문. 비밀번호를 바꾸면 값이 달라져 기존 로그인이 모두 풀린다.
    /// 서명 키로 만든 HMAC이라 쿠키가 새어도 비밀번호를 거꾸로 알아낼 수 없다.
    pub password_fingerprint: String,
}

// 비밀번호와 서명 키는 로그·디버그 출력에 나가지 않게 가린다(설계 1장 8번).
impl std::fmt::Debug for AuthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthConfig")
            .field("password", &"<가림>")
            .field("secret_key", &"<가림>")
            .field("password_fingerprint", &"<가림>")
            .finish()
    }
}

/// 값은 오류 메시지에 넣지 않는다.
pub fn read_auth_config(env: &Env) -> Result<AuthConfig, ConfigError> {
    let password = env.get("APP_PASSWORD").unwrap_or("");
    let secret = env.get("SESSION_SECRET").unwrap_or("");
    if js_len(password) < MIN_PASSWORD_LENGTH {
        return Err(ConfigError(format!(
            "APP_PASSWORD는 {MIN_PASSWORD_LENGTH}자 이상이어야 합니다."
        )));
    }
    if js_len(secret) < MIN_SECRET_LENGTH {
        return Err(ConfigError(format!(
            "SESSION_SECRET은 {MIN_SECRET_LENGTH}자 이상이어야 합니다."
        )));
    }
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC은 어떤 길이의 키도 받는다");
    mac.update(format!("app-password:{password}").as_bytes());
    Ok(AuthConfig {
        password: password.to_string(),
        secret_key: secret.as_bytes().to_vec(),
        password_fingerprint: URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()),
    })
}

/// 길이와 내용에 상관없이 같은 시간에 비교한다(시간 차로 비밀번호를 추측하지 못하게).
pub fn is_password_correct(input: &str, expected: &str) -> bool {
    constant_time_eq_sha256(input.as_bytes(), expected.as_bytes())
}

/// 두 값의 SHA-256을 모든 바이트를 보며 비교한다.
pub fn constant_time_eq_sha256(a: &[u8], b: &[u8]) -> bool {
    let a = Sha256::digest(a);
    let b = Sha256::digest(b);
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
