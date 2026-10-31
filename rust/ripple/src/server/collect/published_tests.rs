use super::*;
use crate::test_support::ms;

#[test]
fn none_or_more_than_ten_minutes_in_the_future_is_seen_time_otherwise_feed_time() {
    // it("없거나 받은 시각보다 10분 넘게 미래면 받은 시각, 아니면 피드 시각")
    let seen = ms("2026-10-22T16:29:00Z");
    let at = |offset: i64| seen + offset;
    assert_eq!(effective_published_at(None, seen), seen);
    // 국민일보 지면 기사: 19시간 미래
    assert_eq!(effective_published_at(Some(at(19 * 3_600_000)), seen), seen);
    assert_eq!(
        effective_published_at(Some(at(FUTURE_TOLERANCE_MS + 1)), seen),
        seen
    );
    // 몇 분 정도는 시계 차이로 보고 그대로 둔다.
    assert_eq!(
        effective_published_at(Some(at(FUTURE_TOLERANCE_MS)), seen),
        at(FUTURE_TOLERANCE_MS)
    );
    assert_eq!(
        effective_published_at(Some(at(-3_600_000)), seen),
        at(-3_600_000)
    );
}
