//! 세션 토큰(`auth/session.ts`). DB 없이 서명한 쿠키로 세션을 표현한다(web은 DB에 쓰지 않음, SPEC D-36).
//! 형식(설계 5.1): `base64url(payload JSON) + "." + base64url(HMAC-SHA256(SESSION_SECRET, payload))`,
//! payload `{"pwd": 비밀번호 지문, "iat": 초, "exp": 초}`. JWT 라이브러리는 쓰지 않는다.
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::Sha256;

use crate::server::auth::config::AuthConfig;
use crate::server::time::EpochMs;

pub const SESSION_COOKIE: &str = "ripple_session";
/// 세션 만료 30일, 접속해도 연장하지 않는다. (SPEC D-10)
pub const SESSION_MAX_AGE_SECONDS: i64 = 30 * 24 * 60 * 60;

fn sign(config: &AuthConfig) -> Hmac<Sha256> {
    Hmac::<Sha256>::new_from_slice(&config.secret_key).expect("HMAC은 어떤 길이의 키도 받는다")
}

pub fn create_session_token(config: &AuthConfig, now: EpochMs) -> String {
    let issued_at = now.div_euclid(1000);
    let payload = json!({
        "pwd": config.password_fingerprint,
        "iat": issued_at,
        "exp": issued_at + SESSION_MAX_AGE_SECONDS,
    })
    .to_string();
    let encoded = URL_SAFE_NO_PAD.encode(payload.as_bytes());
    let mut mac = sign(config);
    mac.update(encoded.as_bytes());
    format!(
        "{encoded}.{}",
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    )
}

/// 서명(시간 차 없는 비교) → 만료 → 비밀번호 지문 순서로 모두 맞아야 유효하다.
pub fn is_valid_session_token(token: Option<&str>, config: &AuthConfig, now: EpochMs) -> bool {
    let Some(token) = token.filter(|t| !t.is_empty()) else {
        return false;
    };
    let Some((encoded, signature)) = token.split_once('.') else {
        return false;
    };
    let Ok(signature) = URL_SAFE_NO_PAD.decode(signature) else {
        return false;
    };
    let mut mac = sign(config);
    mac.update(encoded.as_bytes());
    // `verify_slice`는 시간 차 없이 비교한다.
    if mac.verify_slice(&signature).is_err() {
        return false;
    }
    let Ok(payload) = URL_SAFE_NO_PAD.decode(encoded) else {
        return false;
    };
    let Ok(payload) = serde_json::from_slice::<Value>(&payload) else {
        return false;
    };
    // jose처럼 지금(초 내림)이 exp 이상이면 만료다.
    let Some(exp) = payload.get("exp").and_then(Value::as_i64) else {
        return false;
    };
    if now.div_euclid(1000) >= exp {
        return false;
    }
    payload.get("pwd").and_then(Value::as_str) == Some(config.password_fingerprint.as_str())
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
