//! 설정 표(`settings`) 읽기·쓰기. 키 문자열은 이 파일 한 곳에만 쓴다(설계 3.9).
//! 기본값은 `src/server/settings.ts`의 `SETTING_DEFAULTS`와 같다.
//! 숫자는 모두 f64로 읽고(TS `typeof value === "number"`), 정수로 쓰는 곳은 읽은 뒤 바꾼다.

use rusqlite::{Connection, params};
use serde_json::Value;

use crate::server::text::js_number;
use crate::server::time::EpochMs;

pub struct NumSetting {
    pub key: &'static str,
    pub default: f64,
}

pub struct StrSetting {
    pub key: &'static str,
    pub default: &'static str,
}

/// 글자 목록 설정. 기본값은 빈 목록이다.
pub struct ListSetting {
    pub key: &'static str,
}

const fn num(key: &'static str, default: f64) -> NumSetting {
    NumSetting { key, default }
}

const fn text(key: &'static str, default: &'static str) -> StrSetting {
    StrSetting { key, default }
}

/// 종목 매칭·키워드 생성 모델 (D-02)
pub const LLM_MODEL: StrSetting = text("llm.model", "anthropic/claude-haiku-5.5");
/// 월 예산 한도, 달러 (D-04 임시)
pub const BUDGET_MONTHLY_USD: NumSetting = num("budget.monthlyUsd", 5.0);
/// 키워드 확장 깊이 1~2 (기본 2)
pub const KEYWORDS_DEPTH: NumSetting = num("keywords.depth", 2.0);
/// 기사 평가 모델. 버전을 고정한다(D-23)
pub const JEV_MODEL: StrSetting = text("jev.model", "jev-1.13.0");
/// Jev 단가: 입력 100만 토큰당 달러(D-06, 2026-10-20 공식 가격). 출력은 무료
pub const JEV_USD_PER_MILLION_INPUT_TOKENS: NumSetting = num("jev.usdPerMillionInputTokens", 0.042);
/// 화면에 보일 관련도 기준 0~10 (SPEC FR-5, 기본 6)
pub const EVALUATION_MIN_RELEVANCE: NumSetting = num("evaluation.minRelevance", 6.0);
/// 화면에 보일 영향도 기준 0~10. 관련도와 둘 다 넘어야 보인다(11단계, 17단계에서 기본 5)
pub const EVALUATION_MIN_MAGNITUDE: NumSetting = num("evaluation.minMagnitude", 5.0);
/// 키워드 후보를 남기고 토픽을 종목에 붙이는 기준 0~10(11단계, 기본 관련도 6·영향도 5)
pub const KEYWORDS_MIN_RELEVANCE: NumSetting = num("keywords.minRelevance", 6.0);
pub const KEYWORDS_MIN_MAGNITUDE: NumSetting = num("keywords.minMagnitude", 5.0);
/// 자동 토픽 갱신 간격(시간). 0이면 간격으로는 갱신하지 않는다(11단계, 기본 3)
pub const TOPICS_INTERVAL_HOURS: NumSetting = num("topics.intervalHours", 3.0);
/// 자동 토픽을 갱신할 특정 시각(한국 시간 "HH:MM", 11단계)
pub const TOPICS_TIMES: ListSetting = ListSetting {
    key: "topics.times",
};
/// 자동 토픽을 마지막으로 갱신한 시각(ms). 0이면 아직 없음
pub const TOPICS_REFRESHED_AT: NumSetting = num("topics.refreshedAt", 0.0);
/// 마지막 갱신을 시작한 시각, 마지막으로 실패한 시각과 이유(상태 화면·30분 뒤 다시)
pub const TOPICS_STARTED_AT: NumSetting = num("topics.startedAt", 0.0);
pub const TOPICS_FAILED_AT: NumSetting = num("topics.failedAt", 0.0);
pub const TOPICS_ERROR: StrSetting = text("topics.error", "");
/// 수집 회차 하나에서 평가할 최대 연결 수 (D-03, 기본 200)
pub const EVALUATION_MAX_PER_RUN: NumSetting = num("evaluation.maxPerRun", 200.0);
/// 정기 수집 간격(분): 15·30·60·180 (SPEC FR-6, D-01 기본 30)
pub const COLLECT_INTERVAL_MINUTES: NumSetting = num("collect.intervalMinutes", 30.0);
/// 키워드 자동 재생성 주기 "daily" | "weekly" (SPEC FR-3, 기본 하루 1회)
pub const KEYWORDS_REGENERATE_EVERY: StrSetting = text("keywords.regenerateEvery", "weekly");
/// 키워드 자동 재생성을 마지막으로 한 시각(ms). 0이면 아직 없음
pub const KEYWORDS_AUTO_REGENERATED_AT: NumSetting = num("keywords.autoRegeneratedAt", 0.0);
/// 마지막 자동 백업 시각(ms)과 파일 이름. 0·빈 값이면 아직 없음 (D-27)
pub const BACKUP_LAST_AT: NumSetting = num("backup.lastAt", 0.0);
pub const BACKUP_LAST_FILE: StrSetting = text("backup.lastFile", "");
/// 공식 종목 목록을 마지막으로 받은 시각(ms). 0이면 아직 없음
pub const LISTINGS_SYNCED_AT_KR: NumSetting = num("listings.syncedAt.KR", 0.0);
pub const LISTINGS_SYNCED_AT_US: NumSetting = num("listings.syncedAt.US", 0.0);
/// 주가(13단계): 마지막으로 받기를 마친 시각(ms), 그 회차의 오류(없으면 빈 값), 키가 없어 못 받는 시장 안내
pub const PRICES_CHECKED_AT: NumSetting = num("prices.checkedAt", 0.0);
pub const PRICES_ERROR: StrSetting = text("prices.error", "");
pub const PRICES_NOTE: StrSetting = text("prices.note", "");

/// 저장된 JSON(없거나 깨졌으면 None)
fn read_value(conn: &Connection, key: &str) -> rusqlite::Result<Option<Value>> {
    let mut stmt = conn.prepare_cached("SELECT value FROM settings WHERE key = ?1")?;
    let mut rows = stmt.query(params![key])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let raw: String = row.get(0)?;
    Ok(serde_json::from_str(&raw).ok())
}

/// 저장된 값이 없거나 형식이 다르면 기본값을 쓴다.
pub fn read_num(conn: &Connection, setting: &NumSetting) -> rusqlite::Result<f64> {
    Ok(read_value(conn, setting.key)?
        .and_then(|v| v.as_f64())
        .unwrap_or(setting.default))
}

pub fn read_str(conn: &Connection, setting: &StrSetting) -> rusqlite::Result<String> {
    Ok(match read_value(conn, setting.key)? {
        Some(Value::String(s)) => s,
        _ => setting.default.to_string(),
    })
}

/// 모든 원소가 글자일 때만 쓰고, 아니면 빈 목록
pub fn read_list(conn: &Connection, setting: &ListSetting) -> rusqlite::Result<Vec<String>> {
    Ok(match read_value(conn, setting.key)? {
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| item.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .unwrap_or_default(),
        _ => Vec::new(),
    })
}

/// worker만 부른다(web은 DB에 쓰지 않음, D-36). `now`는 `updated_at`에 넣는 시각이다.
fn write_json(conn: &Connection, key: &str, json: &str, now: EpochMs) -> rusqlite::Result<()> {
    conn.prepare_cached(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = ?3",
    )?
    .execute(params![key, json, now])?;
    Ok(())
}

/// 숫자는 JS `JSON.stringify`와 같은 글자로 쓴다(정수 값 `5.0`은 `5`).
pub fn write_num(
    conn: &Connection,
    setting: &NumSetting,
    value: f64,
    now: EpochMs,
) -> rusqlite::Result<()> {
    write_json(conn, setting.key, &js_number(value), now)
}

pub fn write_str(
    conn: &Connection,
    setting: &StrSetting,
    value: &str,
    now: EpochMs,
) -> rusqlite::Result<()> {
    write_json(conn, setting.key, &json_string(value), now)
}

pub fn write_list(
    conn: &Connection,
    setting: &ListSetting,
    value: &[String],
    now: EpochMs,
) -> rusqlite::Result<()> {
    let items: Vec<String> = value.iter().map(|v| json_string(v)).collect();
    write_json(conn, setting.key, &format!("[{}]", items.join(",")), now)
}

fn json_string(value: &str) -> String {
    // 문자열을 JSON으로 만들 때 실패하는 경우는 없다.
    serde_json::to_string(value).expect("문자열은 항상 JSON이 된다")
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
