//! 평가 읽기(`evaluate/read.ts`). web은 읽기만 한다.
use rusqlite::Connection;

use crate::server::evaluate::run::EVALUATION_WINDOW_MS;
use crate::server::settings::{EVALUATION_MIN_MAGNITUDE, EVALUATION_MIN_RELEVANCE, read_num};
use crate::server::time::EpochMs;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EvaluationTotals {
    pub evaluated: i64,
    pub relevant: i64,
    pub min_relevance: f64,
    pub min_magnitude: f64,
}

/// 평가한 연결 수와 표시 기준(관련도·영향도 설정 값, 11단계) 이상인 수.
/// 표시 기준(`displayRule`): 관련도와 영향도가 둘 다 기준 이상. 11단계 전 평가(영향도 없음)는 관련도만 본다.
pub fn evaluation_totals(conn: &Connection) -> rusqlite::Result<EvaluationTotals> {
    let min_relevance = read_num(conn, &EVALUATION_MIN_RELEVANCE)?;
    let min_magnitude = read_num(conn, &EVALUATION_MIN_MAGNITUDE)?;
    Ok(EvaluationTotals {
        evaluated: conn.query_row("SELECT count(*) FROM evaluations", [], |r| r.get(0))?,
        relevant: conn.query_row(
            "SELECT count(*) FROM evaluations WHERE relevance >= ?1 AND (magnitude IS NULL OR magnitude >= ?2)",
            [min_relevance, min_magnitude],
            |r| r.get(0),
        )?,
        min_relevance,
        min_magnitude,
    })
}

/// 아직 평가하지 않은 최근 7일 기사–종목 연결 수(삭제한 종목·걸러진 연결 제외, D-05와 같은 범위).
/// 키가 없거나, 예산을 다 썼거나, 회차 한도에 밀리면 쌓인다.
pub fn count_pending_evaluations(conn: &Connection, now: EpochMs) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT count(*) FROM article_matches \
         INNER JOIN articles ON articles.id = article_matches.article_id \
         INNER JOIN stocks ON stocks.id = article_matches.stock_id \
         LEFT JOIN evaluations ON evaluations.match_id = article_matches.id \
         WHERE evaluations.id IS NULL AND article_matches.gated = 0 AND stocks.deleted_at IS NULL \
           AND articles.published_at >= ?1",
        [now - EVALUATION_WINDOW_MS],
        |r| r.get(0),
    )
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
