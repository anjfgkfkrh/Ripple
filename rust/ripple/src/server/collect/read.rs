//! 수집 회차 읽기(`collect/read.ts`). web은 읽기만 한다.
use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;

use crate::server::db::schema::RunStatus;
use crate::server::time::EpochMs;

/// 화면에 쓰는 소스 결과. 옛 기록에는 `group`이 없을 수 있어 `label`로 대신한다(`s.group ?? s.label`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSource {
    pub label: String,
    pub group: Option<String>,
    pub fetched: i64,
    pub saved: i64,
    pub matched: i64,
    pub error: Option<String>,
}

/// 회차 안의 Jev 평가 요약(화면에 쓰는 값만)
#[derive(Debug, Clone, PartialEq)]
pub struct RunEvaluation {
    pub evaluated: i64,
    pub relevant: i64,
    pub copied: i64,
    pub failed: i64,
    pub remaining: i64,
    pub cost_usd: f64,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CollectionRun {
    pub status: RunStatus,
    pub started_at: EpochMs,
    pub finished_at: Option<EpochMs>,
    pub since: EpochMs,
    pub sources: Vec<RunSource>,
    pub articles_new: i64,
    pub matches_new: i64,
    pub note: Option<String>,
    pub evaluation: Option<RunEvaluation>,
}

fn int(value: &Value, key: &str) -> i64 {
    value
        .get(key)
        .and_then(Value::as_f64)
        .map_or(0, |v| v as i64)
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn parse_sources(json: &str) -> Vec<RunSource> {
    let Ok(Value::Array(list)) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    list.iter()
        .map(|source| RunSource {
            label: text(source, "label").unwrap_or_default(),
            group: text(source, "group"),
            fetched: int(source, "fetched"),
            saved: int(source, "saved"),
            matched: int(source, "matched"),
            error: text(source, "error"),
        })
        .collect()
}

fn parse_evaluation(json: &str) -> Option<RunEvaluation> {
    let value = serde_json::from_str::<Value>(json).ok()?;
    if !value.is_object() {
        return None;
    }
    Some(RunEvaluation {
        evaluated: int(&value, "evaluated"),
        relevant: int(&value, "relevant"),
        copied: int(&value, "copied"),
        failed: int(&value, "failed"),
        remaining: int(&value, "remaining"),
        cost_usd: value.get("costUsd").and_then(Value::as_f64).unwrap_or(0.0),
        note: text(&value, "note"),
    })
}

/// 가장 최근 수집 회차
pub fn latest_run(conn: &Connection) -> rusqlite::Result<Option<CollectionRun>> {
    conn.query_row(
        "SELECT status, started_at, finished_at, since, sources, articles_new, matches_new, note, evaluation \
         FROM collection_runs ORDER BY id DESC LIMIT 1",
        [],
        |row| {
            let status: String = row.get(0)?;
            let sources: String = row.get(4)?;
            let evaluation: Option<String> = row.get(8)?;
            Ok(CollectionRun {
                status: status.parse().unwrap_or(RunStatus::Failed),
                started_at: row.get(1)?,
                finished_at: row.get(2)?,
                since: row.get(3)?,
                sources: parse_sources(&sources),
                articles_new: row.get(5)?,
                matches_new: row.get(6)?,
                note: row.get(7)?,
                evaluation: evaluation.as_deref().and_then(parse_evaluation),
            })
        },
    )
    .optional()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollectionTotals {
    pub articles: i64,
    pub matches: i64,
}

/// 저장된 기사 수와 기사–종목 연결 수
pub fn collection_totals(conn: &Connection) -> rusqlite::Result<CollectionTotals> {
    Ok(CollectionTotals {
        articles: conn.query_row("SELECT count(*) FROM articles", [], |r| r.get(0))?,
        matches: conn.query_row("SELECT count(*) FROM article_matches", [], |r| r.get(0))?,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureGroup {
    pub reason: String,
    pub labels: Vec<String>,
}

/// 실패한 소스를 같은 이유끼리 묶는다(많은 것 먼저). 회차 전체가 실패해도(인터넷 끊김 등)
/// 상태 화면이 소스 수만큼 길어지지 않게 한다(2026-10-21 결정).
pub fn group_failures(sources: &[RunSource]) -> Vec<FailureGroup> {
    let mut groups: Vec<FailureGroup> = Vec::new();
    for source in sources {
        let Some(error) = source.error.as_deref().filter(|e| !e.is_empty()) else {
            continue;
        };
        match groups.iter_mut().find(|g| g.reason == error) {
            Some(group) => group.labels.push(source.label.clone()),
            None => groups.push(FailureGroup {
                reason: error.to_string(),
                labels: vec![source.label.clone()],
            }),
        }
    }
    // 안정 정렬: 같은 개수면 먼저 나온 이유가 앞(JS Array.sort와 같다)
    groups.sort_by_key(|g| std::cmp::Reverse(g.labels.len()));
    groups
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
