//! 평가 읽기(`evaluate/read.ts`). web은 읽기만 한다.
use rusqlite::Connection;

use crate::server::settings::{EVALUATION_MIN_MAGNITUDE, EVALUATION_MIN_RELEVANCE, read_num};

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
