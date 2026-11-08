use super::*;
use crate::server::auth::config::read_auth_config;
use crate::server::env::Env;
use crate::server::time::now_ms;

const PASSWORD: &str = "correct-horse-battery";
const SECRET: &str = "test-session-secret-0123456789abcdef";
const DAY: i64 = 24 * 60 * 60 * 1000;

fn config_with(password: &str, secret: &str) -> AuthConfig {
    read_auth_config(
        &Env::from_pairs([])
            .with("APP_PASSWORD", password)
            .with("SESSION_SECRET", secret),
    )
    .unwrap()
}

fn config() -> AuthConfig {
    config_with(PASSWORD, SECRET)
}

// it("만든 토큰은 유효하다")
#[test]
fn created_token_is_valid() {
    let now = now_ms();
    let token = create_session_token(&config(), now, 30);
    assert!(is_valid_session_token(Some(&token), &config(), now));
}

// it("만료는 30일: 29일 뒤에는 유효, 31일 뒤에는 무효")
#[test]
fn expires_after_30_days() {
    assert_eq!(SESSION_MAX_AGE_SECONDS, 30 * 24 * 60 * 60);
    let issued_at = 1_792_368_000_000; // 2026-10-19T00:00:00Z
    let token = create_session_token(&config(), issued_at, 30);
    assert!(is_valid_session_token(
        Some(&token),
        &config(),
        issued_at + 29 * DAY
    ));
    assert!(!is_valid_session_token(
        Some(&token),
        &config(),
        issued_at + 31 * DAY
    ));
}

// Rust 전용: 만료 시각 정각(초 내림)부터 무효, 직전 초까지 유효(jose와 같은 경계)
#[test]
fn expiry_boundary_matches_jose() {
    let issued_at = 1_792_368_000_000;
    let token = create_session_token(&config(), issued_at, 30);
    let exp_ms = issued_at + SESSION_MAX_AGE_SECONDS * 1000;
    assert!(is_valid_session_token(Some(&token), &config(), exp_ms - 1));
    assert!(!is_valid_session_token(Some(&token), &config(), exp_ms));
}

// it("서명 키가 다르면 무효")
#[test]
fn other_secret_is_invalid() {
    let now = now_ms();
    let token = create_session_token(&config(), now, 30);
    let other = config_with(PASSWORD, "another-session-secret-0123456789ab");
    assert!(!is_valid_session_token(Some(&token), &other, now));
}

// it("비밀번호를 바꾸면 기존 토큰은 무효 (SPEC FR-1)")
#[test]
fn password_change_invalidates() {
    let now = now_ms();
    let token = create_session_token(&config(), now, 30);
    let changed = config_with("new-correct-horse-battery", SECRET);
    assert!(!is_valid_session_token(Some(&token), &changed, now));
}

// it("빈 값·변조·서명 없는 토큰은 무효")
#[test]
fn empty_tampered_and_unsigned_are_invalid() {
    let now = now_ms();
    let token = create_session_token(&config(), now, 30);
    let (payload, signature) = token.split_once('.').unwrap();
    let tampered = format!("{payload}.{}AA", &signature[..signature.len() - 2]);
    let unsigned = format!("{payload}.");
    assert!(!is_valid_session_token(None, &config(), now));
    assert!(!is_valid_session_token(Some(""), &config(), now));
    assert!(!is_valid_session_token(Some("not-a-token"), &config(), now));
    assert!(!is_valid_session_token(Some(&tampered), &config(), now));
    assert!(!is_valid_session_token(Some(&unsigned), &config(), now));
}

// Rust 전용: 내용을 바꾸고 서명은 그대로 두면 무효
#[test]
fn payload_swap_is_invalid() {
    let now = now_ms();
    let token = create_session_token(&config(), now, 30);
    let (_, signature) = token.split_once('.').unwrap();
    let forged = URL_SAFE_NO_PAD.encode(
        json!({"sub":"owner","role":"owner","edit_exp":i64::MAX / 2000,"pwd": config().password_fingerprint, "iat": 0, "exp": i64::MAX / 2000}).to_string(),
    );
    assert!(!is_valid_session_token(
        Some(&format!("{forged}.{signature}")),
        &config(),
        now
    ));
}

fn sign_payload(payload: Value) -> String {
    let encoded = URL_SAFE_NO_PAD.encode(payload.to_string().as_bytes());
    let mut mac = sign(&config());
    mac.update(encoded.as_bytes());
    format!(
        "{encoded}.{}",
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    )
}

// 새 토큰은 회원이고 edit_exp 전에만 수정 가능
#[test]
fn new_token_is_member_with_edit_window() {
    let now = 1_792_368_000_000;
    let token = create_session_token(&config(), now, 30);
    let access = read_access(Some(&token), &config(), now);
    assert_eq!(
        access,
        Access::Member {
            sub: "owner".into(),
            role: "owner".into(),
            edit_until: now + 30 * 60 * 1000
        }
    );
    assert!(access.can_edit(now));
    let before = now + 30 * 60 * 1000 - 1;
    assert!(read_access(Some(&token), &config(), before).can_edit(before));
    let after = now + 30 * 60 * 1000;
    let late = read_access(Some(&token), &config(), after);
    assert!(late.is_member());
    assert!(!late.can_edit(after));
}

#[test]
fn expired_or_wrong_fingerprint_is_guest() {
    let now = 1_792_368_000_000;
    let token = create_session_token(&config(), now, 30);
    let exp_ms = now + SESSION_MAX_AGE_SECONDS * 1000;
    assert_eq!(read_access(Some(&token), &config(), exp_ms), Access::Guest);
    let other = config_with("another-password-0123456789", SECRET);
    assert_eq!(read_access(Some(&token), &other, now), Access::Guest);
}

#[test]
fn old_token_without_new_fields_is_guest() {
    let now = 1_792_368_000_000;
    let base =
        json!({"pwd": config().password_fingerprint, "iat": now / 1000, "exp": now / 1000 + 1000});
    assert_eq!(
        read_access(Some(&sign_payload(base)), &config(), now),
        Access::Guest
    );
    let full = |k: &str| {
        let mut v = json!({"sub":"owner","role":"owner","edit_exp":now / 1000 + 60,
            "pwd": config().password_fingerprint, "iat": now / 1000, "exp": now / 1000 + 1000});
        v.as_object_mut().unwrap().remove(k);
        v
    };
    for k in ["sub", "role", "edit_exp"] {
        assert_eq!(
            read_access(Some(&sign_payload(full(k))), &config(), now),
            Access::Guest,
            "{k}"
        );
    }
    let bad = json!({"sub":1,"role":"owner","edit_exp":"x","pwd": config().password_fingerprint, "iat": 0, "exp": now / 1000 + 1000});
    assert_eq!(
        read_access(Some(&sign_payload(bad)), &config(), now),
        Access::Guest
    );
}

#[test]
fn bad_signature_is_guest() {
    let now = 1_792_368_000_000;
    let token = create_session_token(&config(), now, 30);
    let other = config_with(PASSWORD, "another-secret-0123456789abcdef-xyz");
    assert_eq!(read_access(Some(&token), &other, now), Access::Guest);
    assert_eq!(read_access(None, &config(), now), Access::Guest);
}
