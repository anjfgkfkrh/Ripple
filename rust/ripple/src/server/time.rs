use chrono::{DateTime, Datelike, NaiveDate};

/// UTC 밀리초. DB와 코드 전체에서 시각은 이 형식으로 다룬다(설계 3.5).
pub type EpochMs = i64;

/// 화면 표시와 스케줄 기준 시간대 (SPEC 4장)
pub const APP_TIME_ZONE: &str = "Asia/Seoul";

/// 한국은 일광절약시간이 없어 항상 UTC+9다.
const SEOUL_OFFSET_MS: i64 = 9 * 60 * 60 * 1000;
pub const DAY_MS: i64 = 24 * 60 * 60 * 1000;

const WEEKDAYS: [&str; 7] = ["일", "월", "화", "수", "목", "금", "토"];

/// 지금(UTC 밀리초). 최상단(worker·web 시작, 요청 처리 시작)에서만 부른다.
pub fn now_ms() -> EpochMs {
    use std::time::{SystemTime, UNIX_EPOCH};
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_millis()).unwrap_or(i64::MAX),
        Err(e) => -i64::try_from(e.duration().as_millis()).unwrap_or(i64::MAX),
    }
}

/// 한국 시간으로 옮긴 시각의 달력 날짜
fn seoul_date(ms: EpochMs) -> NaiveDate {
    DateTime::from_timestamp_millis(ms + SEOUL_OFFSET_MS)
        .expect("시각이 달력 범위를 벗어났습니다")
        .date_naive()
}

fn date_to_utc_ms(date: NaiveDate) -> EpochMs {
    date.and_hms_opt(0, 0, 0)
        .expect("0시는 항상 있다")
        .and_utc()
        .timestamp_millis()
}

/// 이 시각이 속한 날(한국 시간)의 0시
fn start_of_seoul_day(ms: EpochMs) -> EpochMs {
    (ms + SEOUL_OFFSET_MS).div_euclid(DAY_MS) * DAY_MS - SEOUL_OFFSET_MS
}

/// 이 시각이 속한 달(한국 시간 기준)의 1일 0시. 월 예산 합계의 기준이다.
pub fn start_of_seoul_month(ms: EpochMs) -> EpochMs {
    let date = seoul_date(ms);
    let first = NaiveDate::from_ymd_opt(date.year(), date.month(), 1).expect("1일은 항상 있다");
    date_to_utc_ms(first) - SEOUL_OFFSET_MS
}

/// 한국 시간 "YYYY-MM-DD HH:mm:ss"
pub fn format_seoul_date_time(ms: EpochMs) -> String {
    DateTime::from_timestamp_millis(ms + SEOUL_OFFSET_MS)
        .expect("시각이 달력 범위를 벗어났습니다")
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

/// now 이전(같은 시각 포함)의 가장 최근 한국 시간 hour시 정각. weekly면 그중 월요일.
/// 키워드 자동 재생성 시각(매일 06:00, 일주일 1회면 월요일 06:00)의 기준이다.
pub fn last_seoul_boundary(now: EpochMs, hour: i64, weekly: bool) -> EpochMs {
    let mut boundary = start_of_seoul_day(now) + hour * 60 * 60 * 1000;
    if boundary > now {
        boundary -= DAY_MS;
    }
    if weekly {
        // 0=일요일 … 1=월요일. 한국 시간 날짜의 요일로 센다.
        let day = (boundary + SEOUL_OFFSET_MS)
            .div_euclid(DAY_MS)
            .rem_euclid(7)
            + 4;
        boundary -= ((day % 7 + 6) % 7) * DAY_MS;
    }
    boundary
}

/// now 다음(같은 시각 제외)의 한국 시간 hour시 정각. weekly면 그중 월요일.
pub fn next_seoul_boundary(now: EpochMs, hour: i64, weekly: bool) -> EpochMs {
    last_seoul_boundary(now, hour, weekly) + if weekly { 7 } else { 1 } * DAY_MS
}

/// now 이전(같은 시각 포함)의 가장 최근 한국 시간 hour:minute (토픽 갱신 시각, 11단계)
pub fn last_seoul_clock(now: EpochMs, hour: i64, minute: i64) -> EpochMs {
    let mut at = start_of_seoul_day(now) + (hour * 60 + minute) * 60 * 1000;
    if at > now {
        at -= DAY_MS;
    }
    at
}

/// now 다음(같은 시각 제외)의 한국 시간 hour:minute
pub fn next_seoul_clock(now: EpochMs, hour: i64, minute: i64) -> EpochMs {
    last_seoul_clock(now, hour, minute) + DAY_MS
}

/// now 기준 며칠 전인지(한국 시간 날짜로 센다). 오늘 0, 어제 1
fn days_ago(at: EpochMs, now: EpochMs) -> i64 {
    (start_of_seoul_day(now) - start_of_seoul_day(at)) / DAY_MS
}

/// 한국 시간 날짜 "YYYY-MM-DD"(타임라인 날짜 묶음 기준)
pub fn seoul_date_key(ms: EpochMs) -> String {
    format_seoul_date_time(ms)[..10].to_string()
}

/// 타임라인 기사 시각(2026-10-21 결정): "오늘 14:05", "어제 09:12", 그 전은 "10-19 08:00".
/// 해가 다르면 "2025-12-31 08:00".
pub fn format_timeline_time(at: EpochMs, now: EpochMs) -> String {
    let text = format_seoul_date_time(at);
    let time = &text[11..16];
    match days_ago(at, now) {
        0 => format!("오늘 {time}"),
        1 => format!("어제 {time}"),
        _ if text[..4] == format_seoul_date_time(now)[..4] => text[5..16].to_string(),
        _ => text[..16].to_string(),
    }
}

/// 타임라인 날짜 구분선: "오늘 · 10월 21일(수)", "어제 · 10월 20일(화)", "10월 19일(월)". 해가 다르면 연도도
pub fn timeline_day_label(at: EpochMs, now: EpochMs) -> String {
    let date = seoul_date(at);
    let same_year = date.year() == seoul_date(now).year();
    let weekday = WEEKDAYS[date.weekday().num_days_from_sunday() as usize];
    let year = if same_year {
        String::new()
    } else {
        format!("{}년 ", date.year())
    };
    let text = format!("{year}{}월 {}일({weekday})", date.month(), date.day());
    match days_ago(at, now) {
        0 => format!("오늘 · {text}"),
        1 => format!("어제 · {text}"),
        _ => text,
    }
}

#[cfg(test)]
#[path = "time_tests.rs"]
mod tests;
