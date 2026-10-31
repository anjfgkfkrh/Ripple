use std::collections::HashSet;

use super::*;

fn token() -> String {
    "t".repeat(32)
}

fn base() -> Vec<(String, String)> {
    vec![
        ("DATABASE_PATH".into(), "x.db".into()),
        ("WORKER_API_TOKEN".into(), token()),
    ]
}

/// 기본 값(DATABASE_PATH·WORKER_API_TOKEN)에 추가 값을 더한 환경 변수
fn env_with(extra: &[(&str, &str)]) -> Env {
    let mut pairs = base();
    pairs.extend(
        extra
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string())),
    );
    let mut env = Env::default();
    for (k, v) in pairs {
        env = env.with(&k, &v);
    }
    env
}

// describe("readWorkerConfig")
#[test]
fn defaults_use_official_addresses_default_feeds_and_no_keys() {
    // it("기본값: 127.0.0.1:4000, 공식 OpenRouter·목록·Finnhub·TypeSafe 주소, 기본 RSS 목록, 키 없음")
    let env = Env::from_pairs([
        ("DATABASE_PATH", "./data/ripple.db"),
        ("WORKER_API_TOKEN", &"t".repeat(32)),
    ]);
    let config = read_worker_config(&env).unwrap();
    assert_eq!(
        config,
        WorkerConfig {
            database_path: "./data/ripple.db".into(),
            host: "127.0.0.1".into(),
            port: 4000,
            api_token: token(),
            open_router_api_key: None,
            open_router_base_url: "https://openrouter.ai/api/v1".into(),
            listing_sources: listing_sources(),
            finnhub_api_key: None,
            finnhub_base_url: "https://finnhub.io/api/v1".into(),
            rss_feeds: rss_feeds(),
            typesafe_api_key: None,
            typesafe_base_url: "https://api.typesafe.ai".into(),
            scheduler_jobs_enabled: true,
            backup_dir: None,
            kis: None,
            kis_base_url: None,
        }
    );
}

#[test]
fn kis_needs_key_and_secret_defaults_to_demo_and_rejects_bad_kind() {
    // it("KIS(13단계): 키·시크릿이 다 있어야 쓰고, 종류는 기본 모의투자, 잘못 쓰면 시작하지 않는다")
    assert!(
        read_worker_config(&env_with(&[("KIS_APP_KEY", "k")]))
            .unwrap()
            .kis
            .is_none()
    );
    let config = read_worker_config(&env_with(&[
        ("KIS_APP_KEY", " k "),
        ("KIS_APP_SECRET", "s"),
    ]))
    .unwrap();
    assert_eq!(
        config.kis,
        Some(KisCredentials {
            app_key: "k".into(),
            app_secret: "s".into(),
            env: KisEnv::Demo
        })
    );
    let config = read_worker_config(&env_with(&[
        ("KIS_APP_KEY", "k"),
        ("KIS_APP_SECRET", "s"),
        ("KIS_ENV", "REAL"),
        ("KIS_BASE_URL", "http://127.0.0.1:4200/kis"),
    ]))
    .unwrap();
    assert_eq!(config.kis.unwrap().env, KisEnv::Real);
    assert_eq!(
        config.kis_base_url.as_deref(),
        Some("http://127.0.0.1:4200/kis")
    );
    let err = read_worker_config(&env_with(&[
        ("KIS_APP_KEY", "k"),
        ("KIS_APP_SECRET", "s"),
        ("KIS_ENV", "mock"),
    ]))
    .unwrap_err();
    assert!(err.to_string().contains("KIS_ENV"));
}

#[test]
fn backup_dir_is_trimmed() {
    // it("BACKUP_DIR가 있으면 그 폴더에 자동 백업한다")
    let config = read_worker_config(&env_with(&[("BACKUP_DIR", " /backups ")])).unwrap();
    assert_eq!(config.backup_dir.as_deref(), Some("/backups"));
}

#[test]
fn scheduler_disabled_turns_off_jobs() {
    // it("SCHEDULER_DISABLED=1(테스트용)이면 정기 작업을 끈다")
    let enabled = |value: &str| {
        read_worker_config(&env_with(&[("SCHEDULER_DISABLED", value)]))
            .unwrap()
            .scheduler_jobs_enabled
    };
    assert!(!enabled("1"));
    assert!(!enabled("true"));
    assert!(enabled("0"));
}

#[test]
fn source_addresses_can_be_overridden_for_tests() {
    // it("테스트용 원천 주소를 재정의할 수 있다")
    let config = read_worker_config(&env_with(&[
        ("OPENROUTER_API_KEY", " sk-or-1 "),
        ("OPENROUTER_BASE_URL", "http://127.0.0.1:4200/openrouter"),
        ("LISTING_KIND_URL", "http://127.0.0.1:4200/kind"),
    ]))
    .unwrap();
    assert_eq!(config.open_router_api_key.as_deref(), Some("sk-or-1"));
    assert_eq!(
        config.open_router_base_url,
        "http://127.0.0.1:4200/openrouter"
    );
    assert_eq!(config.listing_sources.kind, "http://127.0.0.1:4200/kind");
    assert_eq!(
        config.listing_sources.nasdaq_listed,
        listing_sources().nasdaq_listed
    );
}

#[test]
fn rss_list_and_finnhub_address_can_be_overridden_for_tests() {
    // it("테스트용으로 RSS 목록과 Finnhub 주소를 바꿀 수 있다")
    let feeds = r#"[{"id":"rss:mock:economy","publisher":"가짜경제","section":"경제","url":"http://127.0.0.1:4200/rss/economy.xml"}]"#;
    let config = read_worker_config(&env_with(&[
        ("FINNHUB_API_KEY", " fh "),
        ("FINNHUB_BASE_URL", "http://127.0.0.1:4200/finnhub"),
        ("RSS_FEEDS_JSON", feeds),
    ]))
    .unwrap();
    assert_eq!(config.finnhub_api_key.as_deref(), Some("fh"));
    assert_eq!(config.finnhub_base_url, "http://127.0.0.1:4200/finnhub");
    assert_eq!(
        config.rss_feeds,
        [RssFeed {
            id: "rss:mock:economy".into(),
            publisher: "가짜경제".into(),
            section: "경제".into(),
            url: "http://127.0.0.1:4200/rss/economy.xml".into(),
        }]
    );
}

#[test]
fn typesafe_key_and_address_are_read() {
    // it("TypeSafe 키와 테스트용 주소를 읽는다")
    let config = read_worker_config(&env_with(&[
        ("TYPESAFE_API_KEY", " ts-1 "),
        ("TYPESAFE_BASE_URL", "http://127.0.0.1:4200/typesafe"),
    ]))
    .unwrap();
    assert_eq!(config.typesafe_api_key.as_deref(), Some("ts-1"));
    assert_eq!(config.typesafe_base_url, "http://127.0.0.1:4200/typesafe");
    let blank = read_worker_config(&env_with(&[("TYPESAFE_API_KEY", "  ")])).unwrap();
    assert!(blank.typesafe_api_key.is_none());
}

#[test]
fn bad_rss_feeds_json_stops_startup() {
    // it("RSS_FEEDS_JSON이 잘못되면 시작하지 않는다")
    let err = read_worker_config(&env_with(&[("RSS_FEEDS_JSON", "{")])).unwrap_err();
    assert!(err.to_string().contains("RSS_FEEDS_JSON"));
    let err = read_worker_config(&env_with(&[("RSS_FEEDS_JSON", r#"[{"id":"x"}]"#)])).unwrap_err();
    assert!(err.to_string().contains("RSS_FEEDS_JSON"));
}

#[test]
fn default_feed_list_has_unique_ids_https_urls_and_29_publishers() {
    // it("수집 목록은 id가 겹치지 않고 모두 https 주소다")
    let feeds = rss_feeds();
    let ids: HashSet<_> = feeds.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids.len(), feeds.len());
    assert!(feeds.iter().all(|f| f.url.starts_with("https://")));
    let publishers: HashSet<_> = feeds.iter().map(|f| f.publisher.as_str()).collect();
    assert_eq!(publishers.len(), 29);
}

#[test]
fn missing_worker_token_stops_startup() {
    // it("worker 토큰이 없으면 시작하지 않는다")
    let err = read_worker_config(&Env::from_pairs([("DATABASE_PATH", "x.db")])).unwrap_err();
    assert!(err.to_string().contains("WORKER_API_TOKEN"));
}

#[test]
fn rss_feeds_json_extra_validation_matches_zod() {
    // Rust에서만 생기는 경우: zod 규칙(빈 목록·빈 글자·주소 아님)과 모르는 키 버리기
    for json in [
        "[]",
        r#"[{"id":"","publisher":"p","section":"s","url":"https://a.example/x"}]"#,
        r#"[{"id":"i","publisher":"p","section":"s","url":"not a url"}]"#,
        "null",
    ] {
        assert!(
            read_worker_config(&env_with(&[("RSS_FEEDS_JSON", json)])).is_err(),
            "{json}"
        );
    }
    let json =
        r#"[{"id":"i","publisher":"p","section":"s","url":"https://a.example/x","extra":1}]"#;
    let config = read_worker_config(&env_with(&[("RSS_FEEDS_JSON", json)])).unwrap();
    assert_eq!(config.rss_feeds.len(), 1);
}

#[test]
fn debug_output_hides_secrets() {
    // 설계 1장 규칙 8: Debug 출력에 키가 들어가지 않는다.
    let config = read_worker_config(&env_with(&[
        ("OPENROUTER_API_KEY", "sk-secret-1"),
        ("KIS_APP_KEY", "kis-secret-key"),
        ("KIS_APP_SECRET", "kis-secret-value"),
    ]))
    .unwrap();
    let text = format!("{config:?}");
    for secret in [
        "sk-secret-1",
        "kis-secret-key",
        "kis-secret-value",
        &token(),
    ] {
        assert!(!text.contains(secret), "{secret}");
    }
}
