use super::*;

// TS에는 log.test.ts가 없다. Rust에서 한국 시간 문구 모양을 고정한다.
#[test]
fn format_log_line_uses_seoul_time() {
    // 2026-10-19T00:00:00Z = 한국 09:00
    assert_eq!(
        format_log_line(1_792_368_000_000, "시작"),
        "[2026-10-19 09:00:00] 시작"
    );
}
