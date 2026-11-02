use super::*;

/// 한국 시간 "YYYY-MM-DD HH:MM"을 밀리초로
fn kst(text: &str) -> EpochMs {
    let iso = format!("{}:00+09:00", text.replace(' ', "T"));
    DateTime::parse_from_rfc3339(&iso)
        .unwrap()
        .timestamp_millis()
}

#[test]
fn korea_is_open_weekdays_8_to_20_and_priced_five_minutes_after() {
    // 2026-10-30은 금요일
    assert!(!is_market_open(Market::Kr, kst("2026-10-30 07:59")));
    assert!(is_market_open(Market::Kr, kst("2026-10-30 08:00")));
    assert!(is_market_open(Market::Kr, kst("2026-10-30 19:59")));
    assert!(!is_market_open(Market::Kr, kst("2026-10-30 20:00")));
    assert!(is_price_window(Market::Kr, kst("2026-10-30 20:04")));
    assert!(!is_price_window(Market::Kr, kst("2026-10-30 20:05")));
    // 토·일요일
    assert!(!is_price_window(Market::Kr, kst("2026-10-31 10:00")));
    assert!(!is_price_window(Market::Kr, kst("2026-11-01 10:00")));
}

#[test]
fn us_is_new_york_weekdays_and_follows_daylight_saving_end() {
    // 2026-10-30(금) 뉴욕은 일광절약시간(UTC-4): 한국 22:30에 열고 다음 날 05:00에 닫는다.
    assert!(!is_market_open(Market::Us, kst("2026-10-30 22:29")));
    assert!(is_market_open(Market::Us, kst("2026-10-30 22:30")));
    assert!(is_market_open(Market::Us, kst("2026-10-31 04:59")));
    assert!(!is_market_open(Market::Us, kst("2026-10-31 05:00")));
    assert!(is_price_window(Market::Us, kst("2026-10-31 05:04")));
    // 2026-11-02(월)은 표준시(UTC-5): 한국 23:30에 연다.
    assert!(!is_market_open(Market::Us, kst("2026-11-02 22:30")));
    assert!(is_market_open(Market::Us, kst("2026-11-02 23:30")));
    // 한국 토요일 오전은 뉴욕 금요일 장중, 한국 월요일 오전은 뉴욕 일요일
    assert!(!is_market_open(Market::Us, kst("2026-11-02 02:00")));
}

/// 설계 R8 완료 기준: 뉴욕 일광절약시간이 바뀌는 날(2026-11-01 일요일 02:00 → 01:00, 2026-03-08 일요일 02:00 → 03:00)
/// 전후 평일 개장이 한국 시간으로 한 시간씩 밀리는 것을 경계 분 단위로 본다.
#[test]
fn us_open_shifts_by_one_hour_across_both_daylight_saving_changes() {
    // 가을(끝): 금요일 10-30은 UTC-4(한국 22:30), 월요일 11-02는 UTC-5(한국 23:30)
    assert!(is_market_open(Market::Us, kst("2026-10-30 22:30")));
    assert!(!is_market_open(Market::Us, kst("2026-11-02 22:30")));
    assert!(is_market_open(Market::Us, kst("2026-11-02 23:30")));
    // 겨울 마감은 한국 다음 날 06:00(UTC-5), 거기서 5분 더
    assert!(is_market_open(Market::Us, kst("2026-11-03 05:59")));
    assert!(!is_market_open(Market::Us, kst("2026-11-03 06:00")));
    assert!(is_price_window(Market::Us, kst("2026-11-03 06:04")));
    assert!(!is_price_window(Market::Us, kst("2026-11-03 06:05")));
    // 봄(시작): 금요일 03-06은 UTC-5(한국 23:30), 월요일 03-09는 UTC-4(한국 22:30)
    assert!(!is_market_open(Market::Us, kst("2026-03-06 22:30")));
    assert!(is_market_open(Market::Us, kst("2026-03-06 23:30")));
    assert!(is_market_open(Market::Us, kst("2026-03-09 22:30")));
    assert!(!is_market_open(Market::Us, kst("2026-03-09 22:29")));
    // 일광절약시간이 바뀌는 일요일 자체는 닫혀 있다
    assert!(!is_market_open(Market::Us, kst("2026-11-01 23:30")));
    assert!(!is_market_open(Market::Us, kst("2026-03-08 22:30")));
}
