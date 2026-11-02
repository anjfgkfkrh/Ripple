use std::sync::LazyLock;

use regex::Regex;
use rusqlite::Connection;

use crate::server::settings::{
    TOPICS_ERROR, TOPICS_FAILED_AT, TOPICS_INTERVAL_HOURS, TOPICS_REFRESHED_AT, TOPICS_STARTED_AT,
    TOPICS_TIMES, read_list, read_num, read_str,
};
use crate::server::text::js_trim;
use crate::server::time::{EpochMs, last_seoul_clock, next_seoul_clock};

const HOUR_MS: f64 = 60.0 * 60.0 * 1000.0;

/// 갱신 시각 형식: 한국 시간 "HH:MM"
pub static TOPIC_TIME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([01][0-9]|2[0-3]):([0-5][0-9])$").expect("정규식이 올바르다"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockTime {
    pub hour: i64,
    pub minute: i64,
}

pub fn parse_topic_times(times: &[String]) -> Vec<ClockTime> {
    times
        .iter()
        .filter_map(|time| {
            let caps = TOPIC_TIME.captures(js_trim(time))?;
            Some(ClockTime {
                hour: caps[1].parse().ok()?,
                minute: caps[2].parse().ok()?,
            })
        })
        .collect()
}

/// 갱신이 실패하면(AI 오류·예산 등) 이만큼 뒤에 다시 한다.
pub const TOPIC_RETRY_MS: i64 = 30 * 60 * 1000;
/// 시작하고 이만큼 지나도 끝난 기록이 없으면(worker가 도중에 꺼짐) 갱신 중으로 보지 않는다.
pub const TOPIC_STALE_MS: i64 = 10 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicFailure {
    pub at: EpochMs,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicPlan {
    pub enabled: bool,
    pub due: bool,
    /// 다음 갱신 예정 시각(꺼져 있으면 None). 지금 해야 하면 now
    pub next_at: Option<EpochMs>,
    /// 마지막 갱신 시각(없으면 None)
    pub last_at: Option<EpochMs>,
    /// 지금 갱신하는 중
    pub running: bool,
    /// 마지막 시도가 실패했으면 그 시각과 이유(그 뒤에 성공했으면 None)
    pub failure: Option<TopicFailure>,
}

/// 자동 토픽 갱신 계획(11단계 결정): 사용자가 정한 간격(시간, 기본 3)과 특정 시각(한국 시간) 가운데
/// 먼저 오는 때 한다. 처음이면 바로 한다. 놓친 시각은 켜질 때 한 번만 한다.
/// 실패했으면 30분 뒤에 다시 한다. DB만 읽으므로 상태 화면도 같은 함수로 보여 준다.
pub fn topic_plan(conn: &Connection, now: EpochMs) -> rusqlite::Result<TopicPlan> {
    let interval_hours = read_num(conn, &TOPICS_INTERVAL_HOURS)?;
    let times = parse_topic_times(&read_list(conn, &TOPICS_TIMES)?);
    let refreshed_at = read_num(conn, &TOPICS_REFRESHED_AT)?;
    let started_at = read_num(conn, &TOPICS_STARTED_AT)?;
    let failed_at = read_num(conn, &TOPICS_FAILED_AT)?;
    let now_f = now as f64;
    let last_at = (refreshed_at > 0.0).then_some(refreshed_at as i64);
    let running =
        started_at > refreshed_at.max(failed_at) && now_f - started_at < TOPIC_STALE_MS as f64;
    let failure = if failed_at > refreshed_at {
        Some(TopicFailure {
            at: failed_at as i64,
            error: read_str(conn, &TOPICS_ERROR)?,
        })
    } else {
        None
    };
    if interval_hours <= 0.0 && times.is_empty() {
        return Ok(TopicPlan {
            enabled: false,
            due: false,
            next_at: None,
            last_at,
            running,
            failure,
        });
    }
    let interval_due = interval_hours > 0.0 && now_f - refreshed_at >= interval_hours * HOUR_MS;
    let last_clock = times
        .iter()
        .map(|t| last_seoul_clock(now, t.hour, t.minute))
        .fold(0_i64, i64::max);
    let clock_due = !times.is_empty() && refreshed_at < last_clock as f64;
    if interval_due || clock_due {
        let retry_at = if failure.is_some() {
            failed_at + TOPIC_RETRY_MS as f64
        } else {
            0.0
        };
        if now_f < retry_at {
            return Ok(TopicPlan {
                enabled: true,
                due: false,
                next_at: Some(retry_at as i64),
                last_at,
                running,
                failure,
            });
        }
        return Ok(TopicPlan {
            enabled: true,
            due: !running,
            next_at: Some(now),
            last_at,
            running,
            failure,
        });
    }
    let mut candidates: Vec<f64> = Vec::new();
    if interval_hours > 0.0 {
        candidates.push(refreshed_at + interval_hours * HOUR_MS);
    }
    candidates.extend(
        times
            .iter()
            .map(|t| next_seoul_clock(now, t.hour, t.minute) as f64),
    );
    let next = candidates.into_iter().fold(f64::INFINITY, f64::min);
    Ok(TopicPlan {
        enabled: true,
        due: false,
        next_at: Some(next as i64),
        last_at,
        running,
        failure,
    })
}
