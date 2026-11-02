use rusqlite::{Connection, OptionalExtension};

use crate::server::db::schema::RunTrigger;
use crate::server::settings::{
    COLLECT_INTERVAL_MINUTES, KEYWORDS_AUTO_REGENERATED_AT, KEYWORDS_REGENERATE_EVERY, read_num,
    read_str,
};
use crate::server::time::{EpochMs, last_seoul_boundary, next_seoul_boundary};

/// 고를 수 있는 수집 간격(분) (SPEC FR-6)
pub const COLLECT_INTERVALS: [i64; 4] = [15, 30, 60, 180];
/// 회차 전체가 실패하면 이만큼 뒤에 다시 한다(D-24).
pub const RETRY_AFTER_FAILURE_MS: i64 = 5 * 60 * 1000;
/// 예정 시각보다 이만큼 넘게 늦게 시작하면 '누락 보정'으로 기록한다(worker 꺼짐·노트북 절전).
pub const CATCH_UP_AFTER_MS: i64 = 5 * 60 * 1000;
/// 키워드 자동 재생성 시각(한국 시간, 2026-10-21 결정)
pub const KEYWORD_REGENERATE_HOUR: i64 = 6;

const MINUTE_MS: i64 = 60 * 1000;

/// 저장된 간격(분). 고를 수 없는 값이면 기본값(30)
pub fn read_collect_interval(conn: &Connection) -> rusqlite::Result<i64> {
    let value = read_num(conn, &COLLECT_INTERVAL_MINUTES)?;
    Ok(COLLECT_INTERVALS
        .iter()
        .copied()
        .find(|interval| *interval as f64 == value)
        .unwrap_or(COLLECT_INTERVAL_MINUTES.default as i64))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegenerateEvery {
    Daily,
    Weekly,
}

pub fn read_regenerate_every(conn: &Connection) -> rusqlite::Result<RegenerateEvery> {
    Ok(if read_str(conn, &KEYWORDS_REGENERATE_EVERY)? == "weekly" {
        RegenerateEvery::Weekly
    } else {
        RegenerateEvery::Daily
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionPlan {
    pub interval_minutes: i64,
    /// 등록한 종목이 없어 정기 수집을 쉰다(6단계 결정: 회차를 만들지 않음).
    pub idle: bool,
    /// 지금 도는 회차가 있다.
    pub running: bool,
    /// 다음 정기 수집 시각. 이미 지났으면 다음 확인 때 바로 시작한다.
    pub next_at: EpochMs,
    /// 지난 회차가 모두 실패해 5분 뒤 다시 하는 중(D-24)
    pub retrying: bool,
    pub due: bool,
    pub trigger: RunTrigger,
}

/// 정기 수집 계획(D-31): 마지막으로 성공한 회차(일부 실패 포함, "지금 수집"도 셈) 시작 + 간격.
/// 그 뒤에 회차 전체가 실패했으면 실패한 회차 시작 + 5분과 비교해 늦은 쪽(D-24).
/// DB만 읽으므로 web 화면도 같은 함수로 다음 시각을 보여 준다.
pub fn collection_plan(conn: &Connection, now: EpochMs) -> rusqlite::Result<CollectionPlan> {
    let interval_minutes = read_collect_interval(conn)?;
    // 등록한 종목: 지우지 않은 줄 하나(종류는 가리지 않는다. TS와 같다)
    let idle = conn
        .query_row(
            "SELECT id FROM stocks WHERE deleted_at IS NULL LIMIT 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_none();
    let running = conn
        .query_row(
            "SELECT id FROM collection_runs WHERE status = 'running' LIMIT 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some();
    let last_ok: Option<EpochMs> = conn
        .query_row(
            "SELECT started_at FROM collection_runs WHERE status IN ('success', 'partial') \
             ORDER BY started_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    // lastOk가 없으면 실패한 회차 전부가 대상이다(TS의 `undefined` 조건).
    let last_failed: Option<EpochMs> = conn
        .query_row(
            "SELECT started_at FROM collection_runs WHERE status = 'failed' \
             AND (?1 IS NULL OR started_at > ?1) ORDER BY started_at DESC LIMIT 1",
            [last_ok],
            |row| row.get(0),
        )
        .optional()?;

    let regular = last_ok.map_or(0, |at| at + interval_minutes * MINUTE_MS);
    let retry = last_failed.map_or(0, |at| at + RETRY_AFTER_FAILURE_MS);
    // 한 번도 돈 적이 없으면 바로 시작한다.
    let next = if last_ok.is_some() || last_failed.is_some() {
        regular.max(retry)
    } else {
        now
    };
    Ok(CollectionPlan {
        interval_minutes,
        idle,
        running,
        next_at: next,
        retrying: last_failed.is_some() && retry >= regular,
        due: !idle && !running && now >= next,
        trigger: if now - next > CATCH_UP_AFTER_MS {
            RunTrigger::CatchUp
        } else {
            RunTrigger::Scheduled
        },
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeywordPlan {
    pub every: RegenerateEvery,
    /// 등록한 종목이 없다.
    pub idle: bool,
    /// 이번 주기(06:00)에 아직 다시 만들지 않았으면 지금, 했으면 다음 06:00
    pub next_at: EpochMs,
    pub due: bool,
    /// 지금 다시 만들 종목: 이번 주기 시작 전에 만든 키워드를 가진 종목(만드는 중·대기 중은 뺀다)
    pub stock_ids: Vec<i64>,
}

/// 키워드 자동 재생성 계획(SPEC FR-3, 2026-10-21 결정): 매일 한국 시간 06:00(일주일 1회면 월요일 06:00).
/// 그 시각에 꺼져 있었으면 켜진 뒤 처음 확인할 때 한 번. 주기마다 한 번만 한다(실패해도 다음 주기에 다시).
pub fn keyword_plan(conn: &Connection, now: EpochMs) -> rusqlite::Result<KeywordPlan> {
    let every = read_regenerate_every(conn)?;
    let weekly = every == RegenerateEvery::Weekly;
    let boundary = last_seoul_boundary(now, KEYWORD_REGENERATE_HOUR, weekly);
    let due = read_num(conn, &KEYWORDS_AUTO_REGENERATED_AT)? < boundary as f64;

    struct Active {
        id: i64,
        status: String,
        generated_at: Option<EpochMs>,
    }
    // 자동 토픽 항목은 토픽 갱신이 검색어를 정한다(11단계).
    let mut stmt = conn.prepare(
        "SELECT s.id, s.keywords_status, s.keywords_generated_at FROM stocks s \
         LEFT JOIN topics t ON t.id = s.topic_id \
         WHERE s.deleted_at IS NULL AND (t.source IS NULL OR t.source != 'auto') \
         ORDER BY s.id ASC",
    )?;
    let active: Vec<Active> = stmt
        .query_map([], |row| {
            Ok(Active {
                id: row.get(0)?,
                status: row.get(1)?,
                generated_at: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(KeywordPlan {
        every,
        idle: active.is_empty(),
        next_at: if due {
            now
        } else {
            next_seoul_boundary(now, KEYWORD_REGENERATE_HOUR, weekly)
        },
        due,
        stock_ids: if due {
            active
                .iter()
                .filter(|s| {
                    s.status != "pending"
                        && s.status != "generating"
                        && s.generated_at.is_none_or(|at| at < boundary)
                })
                .map(|s| s.id)
                .collect()
        } else {
            Vec::new()
        },
    })
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
