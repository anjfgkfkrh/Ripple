use super::*;
use crate::server::env::Env;

fn token() -> String {
    "t".repeat(32)
}

// describe("readWorkerApiToken")
#[test]
fn uses_token_of_32_chars_or_more() {
    // it("32자 이상이면 그대로 쓴다")
    let env = Env::from_pairs([("WORKER_API_TOKEN", token().as_str())]);
    assert_eq!(read_worker_api_token(&env).unwrap(), token());
}

#[test]
fn fails_when_missing_or_short_without_leaking_value() {
    // it("없거나 짧으면 실패하고, 값은 메시지에 넣지 않는다")
    let missing = read_worker_api_token(&Env::default()).unwrap_err();
    assert!(missing.0.contains("WORKER_API_TOKEN"));
    let short =
        read_worker_api_token(&Env::from_pairs([("WORKER_API_TOKEN", "short-token")])).unwrap_err();
    assert!(short.0.contains("WORKER_API_TOKEN"));
    assert!(!short.0.contains("short-token"));
}

// describe("isAuthorizedBearer")
#[test]
fn bearer_is_true_only_when_token_matches() {
    // it("Bearer 토큰이 같을 때만 참")
    let token = token();
    assert!(is_authorized_bearer(
        Some(&format!("Bearer {token}")),
        &token
    ));
    assert!(!is_authorized_bearer(
        Some(&format!("Bearer {token}x")),
        &token
    ));
    assert!(!is_authorized_bearer(Some(&token), &token));
    assert!(!is_authorized_bearer(Some("Bearer "), &token));
    assert!(!is_authorized_bearer(None, &token));
}
