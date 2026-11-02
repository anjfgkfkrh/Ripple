use chrono::{DateTime, Datelike, Timelike, Weekday};
use chrono_tz::Tz;

use crate::server::db::schema::Market;
use crate::server::time::EpochMs;

/// 주가를 받는 시간(13단계, 2026-10-30 결정). 휴장일 달력은 두지 않는다(평일이면 받는다).
/// - 한국: 한국 시간 평일 08:00~20:00(통합 시세, 넥스트레이드 프리·애프터마켓 포함)
/// - 미국: 뉴욕 시간 평일 09:30~16:00(정규장, 일광절약시간은 시간대 계산이 맡는다)
///
/// 끝난 뒤 몇 분 더 받아 마지막 체결가를 남긴다.
struct Window {
    time_zone: Tz,
    open: i64,
    close: i64,
}

fn window(market: Market) -> Window {
    match market {
        Market::Kr => Window {
            time_zone: chrono_tz::Asia::Seoul,
            open: 8 * 60,
            close: 20 * 60,
        },
        Market::Us => Window {
            time_zone: chrono_tz::America::New_York,
            open: 9 * 60 + 30,
            close: 16 * 60,
        },
    }
}

/// 장이 끝난 뒤 더 받는 시간(분)
pub const PRICE_CLOSE_GRACE_MINUTES: i64 = 5;

/// 지금 장이 열려 있는지(그 시장 시간 평일, 여는 시각 ≤ 지금 < 닫는 시각)
pub fn is_market_open(market: Market, now: EpochMs) -> bool {
    in_window(market, now, 0)
}

/// 지금 주가를 받을 때인지(장중과 끝난 뒤 몇 분)
pub fn is_price_window(market: Market, now: EpochMs) -> bool {
    in_window(market, now, PRICE_CLOSE_GRACE_MINUTES)
}

fn in_window(market: Market, now: EpochMs, grace: i64) -> bool {
    let window = window(market);
    let Some(utc) = DateTime::from_timestamp_millis(now) else {
        return false;
    };
    let local = utc.with_timezone(&window.time_zone);
    if matches!(local.weekday(), Weekday::Sat | Weekday::Sun) {
        return false;
    }
    let minutes = i64::from(local.hour()) * 60 + i64::from(local.minute());
    minutes >= window.open && minutes < window.close + grace
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
