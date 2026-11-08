use super::*;

// 같은 키로 60번은 통과하고 61번째는 막으며 남은 초를 알려 준다
#[test]
fn blocks_sixty_first_request_with_retry_after() {
    let limiter = GuestRateLimiter::default();
    for _ in 0..MAX_REQUESTS {
        assert_eq!(limiter.check("ip:a", 0), Ok(()));
    }
    assert_eq!(limiter.check("ip:a", 10_000), Err(50));
}

// 1분 창이 지나면 다시 통과한다
#[test]
fn passes_again_after_window() {
    let limiter = GuestRateLimiter::default();
    for _ in 0..MAX_REQUESTS {
        limiter.check("ip:a", 0).unwrap();
    }
    assert!(limiter.check("ip:a", WINDOW_MS - 1).is_err());
    assert_eq!(limiter.check("ip:a", WINDOW_MS), Ok(()));
}

// 다른 키는 따로 센다
#[test]
fn counts_per_key() {
    let limiter = GuestRateLimiter::default();
    for _ in 0..MAX_REQUESTS {
        limiter.check("ip:a", 0).unwrap();
    }
    assert!(limiter.check("ip:a", 1).is_err());
    assert_eq!(limiter.check("ip:b", 1), Ok(()));
}

// 키가 가득 차면 지난 창부터 지우고, 그래도 넘치면 새 키를 거절한다
#[test]
fn evicts_old_windows_then_rejects_new_keys() {
    let limiter = GuestRateLimiter::default();
    for i in 0..10_000 {
        limiter.check(&format!("ip:{i}"), 0).unwrap();
    }
    assert_eq!(limiter.size(), 10_000);
    // 창이 지났으면 지우고 새 키를 받는다
    assert_eq!(limiter.check("ip:new", WINDOW_MS), Ok(()));
    assert_eq!(limiter.size(), 1);
    // 창 안에서 가득 차면 새 키는 거절하고, 이미 있는 키는 계속 센다
    let full = GuestRateLimiter::new(60, WINDOW_MS, 3);
    for i in 0..3 {
        full.check(&format!("ip:{i}"), 0).unwrap();
    }
    assert!(full.check("ip:new", 1_000).is_err());
    assert_eq!(full.size(), 3);
    assert_eq!(full.check("ip:0", 1_000), Ok(()));
}
