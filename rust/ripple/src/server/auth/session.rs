//! 세션 토큰(`auth/session.ts`). DB 없이 서명한 쿠키로 세션을 표현한다(web은 DB에 쓰지 않음, SPEC D-36).
//! 형식(설계 5.1): `base64url(payload JSON) + "." + base64url(HMAC-SHA256(SESSION_SECRET, payload))`,
//! payload `{"sub","role","pwd": 비밀번호 지문, "iat": 초, "exp": 초, "edit_exp": 초}`. JWT 라이브러리는 쓰지 않는다.
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

/// 토큰을 읽은 결과. 계정 기능이 생기면 `sub`·`role`이 오는 곳만 바뀐다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Access {
    Guest,
    Member {
        sub: String,
        role: String,
        edit_until: EpochMs,
    },
}

impl Access {
    pub fn is_member(&self) -> bool {
        matches!(self, Access::Member { .. })
    }

    /// 회원이고 수정 권한 시각 전이면 수정할 수 있다.
    pub fn can_edit(&self, now: EpochMs) -> bool {
        matches!(self, Access::Member { edit_until, .. } if now < *edit_until)
    }
}

pub fn create_session_token(config: &AuthConfig, now: EpochMs, edit_minutes: i64) -> String {
    let issued_at = now.div_euclid(1000);
    let payload = json!({
        "sub": "owner",
        "role": "owner",
        "pwd": config.password_fingerprint,
        "iat": issued_at,
        "exp": issued_at + SESSION_MAX_AGE_SECONDS,
        "edit_exp": issued_at + edit_minutes * 60,
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

/// 서명(시간 차 없는 비교) → 만료 → 비밀번호 지문 → 새 칸 순서로 모두 맞아야 회원이다.
pub fn read_access(token: Option<&str>, config: &AuthConfig, now: EpochMs) -> Access {
    read_member(token, config, now).unwrap_or(Access::Guest)
}

fn read_member(token: Option<&str>, config: &AuthConfig, now: EpochMs) -> Option<Access> {
    let token = token.filter(|t| !t.is_empty())?;
    let (encoded, signature) = token.split_once('.')?;
    let signature = URL_SAFE_NO_PAD.decode(signature).ok()?;
    let mut mac = sign(config);
    mac.update(encoded.as_bytes());
    // `verify_slice`는 시간 차 없이 비교한다.
    mac.verify_slice(&signature).ok()?;
    let payload = URL_SAFE_NO_PAD.decode(encoded).ok()?;
    let payload = serde_json::from_slice::<Value>(&payload).ok()?;
    // jose처럼 지금(초 내림)이 exp 이상이면 만료다.
    let exp = payload.get("exp").and_then(Value::as_i64)?;
    if now.div_euclid(1000) >= exp {
        return None;
    }
    if payload.get("pwd").and_then(Value::as_str) != Some(config.password_fingerprint.as_str()) {
        return None;
    }
    let sub = payload.get("sub").and_then(Value::as_str)?;
    let role = payload.get("role").and_then(Value::as_str)?;
    let edit_exp = payload.get("edit_exp").and_then(Value::as_i64)?;
    Some(Access::Member {
        sub: sub.to_string(),
        role: role.to_string(),
        edit_until: edit_exp.checked_mul(1000)?,
    })
}

pub fn is_valid_session_token(token: Option<&str>, config: &AuthConfig, now: EpochMs) -> bool {
    read_access(token, config, now).is_member()
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
