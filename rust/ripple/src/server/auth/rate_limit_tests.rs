use super::*;

const MINUTE: i64 = 60 * 1000;

// it("4번 틀릴 때까지는 막지 않고, 5번째에 15분 막는다")
#[test]
fn blocks_on_fifth_failure() {
    let limiter = LoginRateLimiter::default();
    for i in 1..MAX_FAILURES as i64 {
        assert_eq!(limiter.record_failure("ip:a", i), 0);
        assert_eq!(limiter.blocked_for("ip:a", i), 0);
    }
    assert_eq!(
        limiter.record_failure("ip:a", MAX_FAILURES as i64),
        BLOCK_MS
    );
    assert_eq!(
        limiter.blocked_for("ip:a", MAX_FAILURES as i64 + 1),
        BLOCK_MS - 1
    );
}

// it("15분이 지나면 풀리고, 다시 5번 틀려야 막힌다")
#[test]
fn unblocks_after_15_minutes() {
    let limiter = LoginRateLimiter::default();
    for _ in 0..MAX_FAILURES {
        limiter.record_failure("ip:a", 0);
    }
    assert_eq!(limiter.blocked_for("ip:a", BLOCK_MS - 1), 1);
    assert_eq!(limiter.blocked_for("ip:a", BLOCK_MS), 0);
    assert_eq!(limiter.record_failure("ip:a", BLOCK_MS), 0);
}

// it("15분보다 오래된 실패는 세지 않는다")
#[test]
fn old_failures_do_not_count() {
    let limiter = LoginRateLimiter::default();
    for _ in 0..MAX_FAILURES - 1 {
        limiter.record_failure("ip:a", 0);
    }
    assert_eq!(limiter.record_failure("ip:a", 16 * MINUTE), 0);
}

// it("IP마다 따로 세고, 성공하면 기록을 지운다")
#[test]
fn counts_per_key_and_reset_clears() {
    let limiter = LoginRateLimiter::default();
    for _ in 0..MAX_FAILURES {
        limiter.record_failure("ip:a", 0);
    }
    assert!(limiter.blocked_for("ip:a", 1) > 0);
    assert_eq!(limiter.blocked_for("ip:b", 1), 0);

    for _ in 0..MAX_FAILURES - 1 {
        limiter.record_failure("ip:c", 0);
    }
    limiter.reset("ip:c");
    assert_eq!(limiter.record_failure("ip:c", 1), 0);
}

// it("기록이 너무 많아지면 끝난 기록부터 정리하고, 아직 막힌 기록은 남긴다")
#[test]
fn prunes_finished_records_only() {
    let limiter = LoginRateLimiter::new(MAX_FAILURES, BLOCK_MS, BLOCK_MS, 3);
    limiter.record_failure("ip:old1", 0);
    limiter.record_failure("ip:old2", 0);
    // 10분에 막힘 → 25분까지 차단
    for _ in 0..MAX_FAILURES {
        limiter.record_failure("ip:blocked", 10 * MINUTE);
    }
    assert_eq!(limiter.size(), 3);

    // 16분: old1·old2는 15분이 지나 끝난 기록이라 정리된다.
    limiter.record_failure("ip:new", 16 * MINUTE);
    assert_eq!(limiter.size(), 2);
    assert_eq!(limiter.blocked_for("ip:blocked", 16 * MINUTE), 9 * MINUTE);
}
