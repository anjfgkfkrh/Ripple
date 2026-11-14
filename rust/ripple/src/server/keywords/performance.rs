use std::collections::HashMap;

use rusqlite::{Connection, params};

use crate::server::db::schema::BYSTANDER_THRESHOLD;
use crate::server::evaluate::run::{NOT_HIDDEN_SQL, meets_display_rule, not_nature_hidden_sql};
use crate::server::keywords::tree::{PATH_SEPARATOR, normalize_key};
use crate::server::time::EpochMs;

/// 키워드 글자 하나의 실적(17단계 결정: 정기 점검이 뺄 것을 지킬 때 쓴다)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeywordPerformance {
    /// 걸린 연결 수(걸러진 `gated` 연결도 센다)
    pub matched: usize,
    /// 그중 평가가 화면 표시 기준을 넘은 수
    pub passed: usize,
    /// 마지막으로 통과한 연결의 시각
    pub last_passed_at: Option<EpochMs>,
}

/// 종목의 키워드 실적. `since`(연결을 만든 시각이 이 값 이상인 것만 센다) 이후 `match_kind = 'keyword'` 연결을
/// 경로(`keyword_path`)의 마지막 낱말로 묶는다. 키워드를 다시 만들어 경로가 바뀌어도 마지막 낱말이 같으면 이어서 센다.
/// 돌려주는 표의 키는 `normalize_key`한 키워드 글자다(공백·대소문자를 가리지 않는다).
/// 통과는 `meets_display_rule`(화면 표시 기준)과 같다. 평가가 없는 연결은 걸린 수에만 센다.
pub fn keyword_performance(
    conn: &Connection,
    stock_id: i64,
    since: EpochMs,
) -> rusqlite::Result<HashMap<String, KeywordPerformance>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT article_matches.keyword_path, article_matches.created_at, \
                evaluations.relevance, evaluations.magnitude, \
                CASE WHEN {NOT_HIDDEN_SQL} AND {nature} THEN 1 ELSE 0 END \
         FROM article_matches \
         LEFT JOIN evaluations ON evaluations.match_id = article_matches.id \
         INNER JOIN articles ON articles.id = article_matches.article_id \
         INNER JOIN stocks ON stocks.id = article_matches.stock_id \
         WHERE article_matches.stock_id = ?1 AND article_matches.match_kind = 'keyword' \
           AND article_matches.created_at >= ?2",
        NOT_HIDDEN_SQL = NOT_HIDDEN_SQL.replace('?', "?3"),
        nature = not_nature_hidden_sql()
    ))?;
    let rows = stmt
        .query_map(params![stock_id, since, BYSTANDER_THRESHOLD], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, EpochMs>(1)?,
                row.get::<_, Option<f64>>(2)?,
                row.get::<_, Option<f64>>(3)?,
                row.get::<_, i64>(4)? == 1,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut table: HashMap<String, KeywordPerformance> = HashMap::new();
    for (path, created_at, relevance, magnitude, not_hidden) in rows {
        let Some(last) = path
            .as_deref()
            .and_then(|p| p.rsplit(PATH_SEPARATOR).next())
        else {
            continue;
        };
        let entry = table.entry(normalize_key(last)).or_default();
        entry.matched += 1;
        if let Some(relevance) = relevance
            && not_hidden
            && meets_display_rule(conn, relevance, magnitude)?
        {
            entry.passed += 1;
            entry.last_passed_at = entry.last_passed_at.max(Some(created_at));
        }
    }
    Ok(table)
}

#[cfg(test)]
#[path = "performance_tests.rs"]
mod tests;
