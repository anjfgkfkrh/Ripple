use std::collections::HashMap;

use rusqlite::{Connection, Row, params_from_iter};

use crate::server::db::schema::KeywordKind;
use crate::server::keywords::tree::clamp_depth;
use crate::server::settings::{KEYWORDS_DEPTH, read_num};
use crate::server::time::EpochMs;

/// `keywords` 표의 한 줄
#[derive(Debug, Clone, PartialEq)]
pub struct Keyword {
    pub id: i64,
    pub stock_id: i64,
    pub level: i64,
    pub parent_id: Option<i64>,
    pub text_ko: String,
    pub text_en: Option<String>,
    pub reason: String,
    pub path: String,
    pub created_at: EpochMs,
    pub kind: KeywordKind,
    pub relevance: Option<f64>,
    pub magnitude: Option<f64>,
    pub topic_id: Option<i64>,
}

const KEYWORD_COLUMNS: &str = "id, stock_id, level, parent_id, text_ko, text_en, reason, path, created_at, kind, relevance, magnitude, topic_id";

fn row_to_keyword(row: &Row<'_>) -> rusqlite::Result<Keyword> {
    let kind: String = row.get(9)?;
    Ok(Keyword {
        id: row.get(0)?,
        stock_id: row.get(1)?,
        level: row.get(2)?,
        parent_id: row.get(3)?,
        text_ko: row.get(4)?,
        text_en: row.get(5)?,
        reason: row.get(6)?,
        path: row.get(7)?,
        created_at: row.get(8)?,
        kind: kind.parse().map_err(|e: String| {
            rusqlite::Error::FromSqlConversionFailure(9, rusqlite::types::Type::Text, e.into())
        })?,
        relevance: row.get(10)?,
        magnitude: row.get(11)?,
        topic_id: row.get(12)?,
    })
}

/// 종목별 키워드(단계·생성 순서대로). web은 읽기만 한다. 키워드가 없는 종목도 빈 목록으로 들어간다.
pub fn list_keywords_by_stock(
    conn: &Connection,
    stock_ids: &[i64],
) -> rusqlite::Result<HashMap<i64, Vec<Keyword>>> {
    let mut grouped: HashMap<i64, Vec<Keyword>> =
        stock_ids.iter().map(|id| (*id, Vec::new())).collect();
    if stock_ids.is_empty() {
        return Ok(grouped);
    }
    let marks = vec!["?"; stock_ids.len()].join(",");
    let mut stmt = conn.prepare(&format!(
        "SELECT {KEYWORD_COLUMNS} FROM keywords WHERE stock_id IN ({marks}) ORDER BY level ASC, id ASC"
    ))?;
    let rows = stmt.query_map(params_from_iter(stock_ids), row_to_keyword)?;
    for row in rows {
        let keyword = row?;
        if let Some(list) = grouped.get_mut(&keyword.stock_id) {
            list.push(keyword);
        }
    }
    Ok(grouped)
}

/// 저장한 깊이와 다른 깊이로 키워드를 만든 종목(키워드가 없는 종목 포함). 만드는 중·대기 중인 종목은 뺀다.
/// 설정 화면의 "지금 다시 만들기"가 이 종목들을 다시 만든다(2026-10-21 결정).
/// 11단계: 트리를 만들지 않는 자동 토픽 항목은 빼고, 토픽 갱신이 붙인 가지는 깊이에 넣지 않는다.
pub fn stocks_with_other_depth(conn: &Connection) -> rusqlite::Result<Vec<i64>> {
    let depth = clamp_depth(read_num(conn, &KEYWORDS_DEPTH)?);
    let mut stmt = conn.prepare(
        "SELECT stocks.id, stocks.keywords_status, max(keywords.level) FROM stocks \
         LEFT JOIN keywords ON keywords.stock_id = stocks.id AND keywords.topic_id IS NULL \
         LEFT JOIN topics ON topics.id = stocks.topic_id \
         WHERE stocks.deleted_at IS NULL AND (topics.source IS NULL OR topics.source != 'auto') \
         GROUP BY stocks.id ORDER BY stocks.id ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<i64>>(2)?,
        ))
    })?;
    let mut ids = Vec::new();
    for row in rows {
        let (id, status, deepest) = row?;
        if status != "pending" && status != "generating" && deepest.unwrap_or(0) != depth {
            ids.push(id);
        }
    }
    Ok(ids)
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
