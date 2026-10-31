use super::*;

/// 테스트 보조: ISO 8601 UTC 글자("2026-10-19T00:00:00Z") → 밀리초. TS의 `new Date(iso)`.
fn at(iso: &str) -> EpochMs {
    DateTime::parse_from_rfc3339(iso)
        .unwrap()
        .timestamp_millis()
}

/// 테스트 보조: 밀리초 → "2026-09-30T15:00:00.000Z". TS의 `toISOString()`.
fn iso(ms: EpochMs) -> String {
    DateTime::from_timestamp_millis(ms)
        .unwrap()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

// describe("startOfSeoulMonth")
#[test]
fn start_of_seoul_month_returns_first_day_midnight_in_seoul() {
    // it("한국 시간 기준 그달 1일 0시를 돌려준다")
    assert_eq!(
        iso(start_of_seoul_month(at("2026-10-20T03:00:00Z"))),
        "2026-09-30T15:00:00.000Z"
    );
}

#[test]
fn start_of_seoul_month_uses_seoul_month_not_utc() {
    // it("UTC로는 전달이어도 한국 시간으로 새달이면 새달 1일")
    // 2026-10-31T16:00Z = 한국 2026-11-01 01:00
    assert_eq!(
        iso(start_of_seoul_month(at("2026-10-31T16:00:00Z"))),
        "2026-10-31T15:00:00.000Z"
    );
    // 2026-10-31T14:59Z = 한국 2026-10-31 23:59 → 아직 10월
    assert_eq!(
        iso(start_of_seoul_month(at("2026-10-31T14:59:00Z"))),
        "2026-09-30T15:00:00.000Z"
    );
}

#[test]
fn start_of_seoul_month_crosses_year_end() {
    // it("연말도 넘어간다")
    assert_eq!(
        iso(start_of_seoul_month(at("2026-12-31T15:30:00Z"))),
        "2026-12-31T15:00:00.000Z"
    );
}

// describe("formatSeoulDateTime")
#[test]
fn format_seoul_date_time_adds_nine_hours() {
    // it("UTC 시각을 한국 시간(+9시간)으로 표시한다")
    assert_eq!(
        format_seoul_date_time(at("2026-10-19T00:00:00Z")),
        "2026-10-19 09:00:00"
    );
}

#[test]
fn format_seoul_date_time_handles_date_change_and_midnight() {
    // it("날짜가 넘어가는 경우와 자정을 00시로 표시한다")
    assert_eq!(
        format_seoul_date_time(at("2026-12-31T15:00:00Z")),
        "2027-01-01 00:00:00"
    );
    assert_eq!(
        format_seoul_date_time(at("2026-03-05T14:59:59Z")),
        "2026-03-05 23:59:59"
    );
}

// describe("lastSeoulBoundary / nextSeoulBoundary (키워드 재생성 06:00)")
// 2026-10-21은 수요일, 2026-10-19는 월요일
#[test]
fn daily_boundary_is_today_0600_or_yesterday_0600() {
    // it("매일: 한국 06:00 전이면 전날 06:00, 06:00부터는 그날 06:00")
    // 한국 10-21 05:59
    assert_eq!(
        iso(last_seoul_boundary(at("2026-10-20T20:59:00Z"), 6, false)),
        "2026-10-19T21:00:00.000Z"
    );
    // 한국 10-21 06:00 정각
    assert_eq!(
        iso(last_seoul_boundary(at("2026-10-20T21:00:00Z"), 6, false)),
        "2026-10-20T21:00:00.000Z"
    );
    // 한국 10-21 23:30
    assert_eq!(
        iso(last_seoul_boundary(at("2026-10-21T14:30:00Z"), 6, false)),
        "2026-10-20T21:00:00.000Z"
    );
    assert_eq!(
        iso(next_seoul_boundary(at("2026-10-21T14:30:00Z"), 6, false)),
        "2026-10-21T21:00:00.000Z"
    );
}

#[test]
fn weekly_boundary_is_latest_monday_0600() {
    // it("일주일: 가장 최근 월요일 06:00")
    // 한국 10-21(수) 12:00 → 10-19(월) 06:00
    assert_eq!(
        iso(last_seoul_boundary(at("2026-10-21T03:00:00Z"), 6, true)),
        "2026-10-18T21:00:00.000Z"
    );
    // 한국 10-19(월) 05:00 → 그 전 주 월요일 10-12 06:00
    assert_eq!(
        iso(last_seoul_boundary(at("2026-10-18T20:00:00Z"), 6, true)),
        "2026-10-11T21:00:00.000Z"
    );
    // 한국 10-19(월) 06:00 정각 → 그날
    assert_eq!(
        iso(last_seoul_boundary(at("2026-10-18T21:00:00Z"), 6, true)),
        "2026-10-18T21:00:00.000Z"
    );
    // 한국 10-25(일) 23:00 → 다음은 10-26(월) 06:00
    assert_eq!(
        iso(next_seoul_boundary(at("2026-10-25T14:00:00Z"), 6, true)),
        "2026-10-25T21:00:00.000Z"
    );
}

// describe("타임라인 시각 (2026-10-21 결정)")
// 한국 시간 2026-10-21(수) 00:30
const NOW: &str = "2026-10-20T15:30:00Z";

#[test]
fn timeline_time_formats_today_yesterday_and_older() {
    // it("오늘·어제는 '오늘 14:05'처럼, 그 전은 '10-19 08:00', 해가 다르면 연도까지")
    let now = at(NOW);
    // 한국 10-21 00:10
    assert_eq!(
        format_timeline_time(at("2026-10-20T15:10:00Z"), now),
        "오늘 00:10"
    );
    // 한국 10-20 23:59
    assert_eq!(
        format_timeline_time(at("2026-10-20T14:59:00Z"), now),
        "어제 23:59"
    );
    // 한국 10-19 08:00
    assert_eq!(
        format_timeline_time(at("2026-10-18T23:00:00Z"), now),
        "10-19 08:00"
    );
    assert_eq!(
        format_timeline_time(at("2025-12-31T00:00:00Z"), at("2026-01-02T00:00:00Z")),
        "2025-12-31 09:00"
    );
}

#[test]
fn timeline_day_label_names_today_yesterday_and_weekday() {
    // it("날짜 구분선은 오늘·어제를 붙이고 요일을 쓴다")
    let now = at(NOW);
    assert_eq!(
        timeline_day_label(at("2026-10-20T15:10:00Z"), now),
        "오늘 · 10월 21일(수)"
    );
    assert_eq!(
        timeline_day_label(at("2026-10-20T14:59:00Z"), now),
        "어제 · 10월 20일(화)"
    );
    assert_eq!(
        timeline_day_label(at("2026-10-18T23:00:00Z"), now),
        "10월 19일(월)"
    );
    assert_eq!(
        timeline_day_label(at("2025-12-31T00:00:00Z"), at("2026-01-02T00:00:00Z")),
        "2025년 12월 31일(수)"
    );
    assert_eq!(seoul_date_key(at("2026-10-20T15:10:00Z")), "2026-10-21");
}

// describe("lastSeoulClock·nextSeoulClock (11단계 토픽 갱신 시각)")
#[test]
fn seoul_clock_finds_last_and_next_time_of_day() {
    // it("지금 이전(같은 시각 포함)의 마지막 한국 시간 HH:MM과 그다음")
    // 한국 시간 2026-10-22 15:00
    let now = at("2026-10-22T06:00:00Z");
    assert_eq!(last_seoul_clock(now, 8, 30), at("2026-10-21T23:30:00Z"));
    assert_eq!(last_seoul_clock(now, 15, 0), now);
    assert_eq!(last_seoul_clock(now, 15, 1), at("2026-10-21T06:01:00Z"));
    assert_eq!(next_seoul_clock(now, 15, 0), at("2026-10-23T06:00:00Z"));
    assert_eq!(next_seoul_clock(now, 23, 59), at("2026-10-22T14:59:00Z"));
    // 한국 자정 직후(UTC로는 전날)
    assert_eq!(
        next_seoul_clock(at("2026-10-22T15:10:00Z"), 0, 5),
        at("2026-10-23T15:05:00Z")
    );
}

#[test]
fn now_ms_is_after_2026() {
    // Rust에서만 필요한 점검: 시스템 시계를 밀리초로 읽는다.
    assert!(now_ms() > at("2026-01-01T00:00:00Z"));
}
