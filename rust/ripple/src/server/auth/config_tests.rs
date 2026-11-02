use super::*;

const PASSWORD: &str = "correct-horse-battery";

fn secret() -> String {
    "s".repeat(MIN_SECRET_LENGTH)
}

fn env(password: Option<&str>, secret: Option<&str>) -> Env {
    let mut env = Env::from_pairs([]);
    if let Some(password) = password {
        env = env.with("APP_PASSWORD", password);
    }
    if let Some(secret) = secret {
        env = env.with("SESSION_SECRET", secret);
    }
    env
}

mod read_auth_config {
    use super::*;

    // it("비밀번호·서명 키가 충분히 길면 설정을 만든다")
    #[test]
    fn builds_config_when_long_enough() {
        let config = read_auth_config(&env(Some(PASSWORD), Some(&secret()))).unwrap();
        assert_eq!(config.password, PASSWORD);
        assert_eq!(config.secret_key.len(), MIN_SECRET_LENGTH);
    }

    // it.each("없거나 짧으면 실패한다 (%o)")
    #[test]
    fn fails_when_missing_or_short() {
        let short_password = "x".repeat(MIN_PASSWORD_LENGTH - 1);
        let short_secret = "s".repeat(MIN_SECRET_LENGTH - 1);
        let cases = [
            (env(None, Some(&secret())), "APP_PASSWORD"),
            (env(Some(&short_password), Some(&secret())), "APP_PASSWORD"),
            (env(Some(PASSWORD), None), "SESSION_SECRET"),
            (env(Some(PASSWORD), Some(&short_secret)), "SESSION_SECRET"),
        ];
        for (env, name) in cases {
            let error = read_auth_config(&env).unwrap_err();
            assert!(error.0.contains(name), "{} / {name}", error.0);
        }
    }

    // it("오류 메시지에 값을 넣지 않는다")
    #[test]
    fn error_message_has_no_value() {
        let short_secret = "short-secret-value";
        let error = read_auth_config(&env(Some(PASSWORD), Some(short_secret))).unwrap_err();
        assert!(!error.0.contains(short_secret));
    }

    // it("비밀번호를 바꾸면 지문이 바뀌고, 지문에 비밀번호가 그대로 들어가지 않는다")
    #[test]
    fn fingerprint_changes_with_password() {
        let a = read_auth_config(&env(Some(PASSWORD), Some(&secret()))).unwrap();
        let b = read_auth_config(&env(Some(&format!("{PASSWORD}!")), Some(&secret()))).unwrap();
        assert_ne!(a.password_fingerprint, b.password_fingerprint);
        assert!(!a.password_fingerprint.contains(PASSWORD));
    }

    // Rust 전용: Debug 출력에 비밀 값이 나가지 않는다(설계 1장 8번)
    #[test]
    fn debug_output_hides_secrets() {
        let config = read_auth_config(&env(Some(PASSWORD), Some(&secret()))).unwrap();
        let text = format!("{config:?}");
        assert!(!text.contains(PASSWORD));
        assert!(!text.contains(&config.password_fingerprint));
    }
}

mod minimum_length {
    use super::*;

    // it("비밀번호는 7자부터 받고 6자는 거부한다")
    #[test]
    fn password_from_seven_chars() {
        assert_eq!(MIN_PASSWORD_LENGTH, 7);
        assert!(read_auth_config(&env(Some("1234567"), Some(&secret()))).is_ok());
        let error = read_auth_config(&env(Some("123456"), Some(&secret()))).unwrap_err();
        assert!(error.0.contains("APP_PASSWORD는 7자 이상"));
    }

    // it("서명 키는 32자부터 받는다")
    #[test]
    fn secret_from_32_chars() {
        assert_eq!(MIN_SECRET_LENGTH, 32);
    }
}

// describe("isPasswordCorrect") — it("같으면 참, 다르면(길이가 달라도) 거짓")
#[test]
fn password_comparison() {
    assert!(is_password_correct(PASSWORD, PASSWORD));
    assert!(!is_password_correct(&format!("{PASSWORD} "), PASSWORD));
    assert!(!is_password_correct("", PASSWORD));
    assert!(!is_password_correct("x", PASSWORD));
}
