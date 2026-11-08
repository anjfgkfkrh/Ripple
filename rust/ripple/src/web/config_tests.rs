use super::*;

fn base() -> Env {
    Env::from_pairs([
        ("APP_PASSWORD", "correct-horse-battery"),
        ("SESSION_SECRET", "test-session-secret-0123456789abcdef"),
        ("WORKER_API_TOKEN", "worker-test-token-0123456789abcdef"),
    ])
}

// Rust 전용: 기본값(0.0.0.0:3000)과 환경 변수로 바꾼 값
#[test]
fn reads_defaults_and_overrides() {
    let config = read_web_config(&base()).unwrap();
    assert_eq!((config.host.as_str(), config.port), ("0.0.0.0", 3000));
    assert_eq!(config.database_path, None);

    let env = base()
        .with("HOSTNAME", "127.0.0.1")
        .with("PORT", "3100")
        .with("DATABASE_PATH", "/data/ripple.db")
        .with("WORKER_URL", "http://worker:4000")
        .with("TUNNEL_METRICS_URL", " ");
    let config = read_web_config(&env).unwrap();
    assert_eq!((config.host.as_str(), config.port), ("127.0.0.1", 3100));
    assert_eq!(config.database_path.as_deref(), Some("/data/ripple.db"));
    assert_eq!(config.worker_url.as_deref(), Some("http://worker:4000"));
    assert_eq!(config.tunnel_metrics_url, None);
}

// 16단계: GUEST_VIEW — 비었음·on → 켬, off → 끔, 그 밖 → 오류(값은 메시지에 없음)
#[test]
fn reads_guest_view() {
    assert!(read_web_config(&base()).unwrap().guest_view);
    assert!(
        read_web_config(&base().with("GUEST_VIEW", " "))
            .unwrap()
            .guest_view
    );
    assert!(
        read_web_config(&base().with("GUEST_VIEW", "on"))
            .unwrap()
            .guest_view
    );
    assert!(
        !read_web_config(&base().with("GUEST_VIEW", "off"))
            .unwrap()
            .guest_view
    );
    let err = read_web_config(&base().with("GUEST_VIEW", "SECRETVAL")).unwrap_err();
    assert!(err.0.contains("GUEST_VIEW") && !err.0.contains("SECRETVAL"));
}

// 16단계: EDIT_UNLOCK_MINUTES — 비었음 30, 1~120 받음, 그 밖 → 오류(값은 메시지에 없음)
#[test]
fn reads_edit_unlock_minutes() {
    let minutes = |v: &str| read_web_config(&base().with("EDIT_UNLOCK_MINUTES", v));
    assert_eq!(read_web_config(&base()).unwrap().edit_unlock_minutes, 30);
    assert_eq!(minutes(" ").unwrap().edit_unlock_minutes, 30);
    assert_eq!(minutes("1").unwrap().edit_unlock_minutes, 1);
    assert_eq!(minutes("120").unwrap().edit_unlock_minutes, 120);
    for bad in ["0", "121", "abc"] {
        let err = minutes(bad).unwrap_err();
        assert!(err.0.contains("EDIT_UNLOCK_MINUTES"));
        assert!(!err.0.contains(&format!(": {bad}")) && !err.0.contains(&format!("{bad}입")));
    }
}

// Rust 전용: PUBLIC_URL은 앞뒤 공백을 떼고, 빈 값은 없음으로 읽는다(15단계)
#[test]
fn reads_public_url_trimmed() {
    assert_eq!(read_web_config(&base()).unwrap().public_url, None);
    let env = base().with("PUBLIC_URL", "  https://example.com ");
    assert_eq!(
        read_web_config(&env).unwrap().public_url.as_deref(),
        Some("https://example.com")
    );
    let env = base().with("PUBLIC_URL", "  ");
    assert_eq!(read_web_config(&env).unwrap().public_url, None);
}

// Rust 전용: instrumentation-node.ts — 비밀 값이 없거나 짧으면 시작하지 않는다(값은 오류에 넣지 않는다)
#[test]
fn refuses_missing_or_short_secrets() {
    let no_password = Env::from_pairs([
        ("SESSION_SECRET", "test-session-secret-0123456789abcdef"),
        ("WORKER_API_TOKEN", "worker-test-token-0123456789abcdef"),
    ]);
    assert!(
        read_web_config(&no_password)
            .unwrap_err()
            .0
            .contains("APP_PASSWORD")
    );
    let short_token = base().with("WORKER_API_TOKEN", "short-token-value");
    let error = read_web_config(&short_token).unwrap_err();
    assert!(error.0.contains("WORKER_API_TOKEN"));
    assert!(!error.0.contains("short-token-value"));
}

// Rust 전용: Debug 출력에 비밀 값이 나가지 않는다
#[test]
fn debug_hides_secrets() {
    let text = format!("{:?}", read_web_config(&base()).unwrap());
    assert!(!text.contains("correct-horse-battery"));
    assert!(!text.contains("worker-test-token"));
}
