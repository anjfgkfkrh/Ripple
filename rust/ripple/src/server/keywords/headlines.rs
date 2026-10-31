use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use rusqlite::{Connection, params};

use crate::server::settings::{EVALUATION_MIN_RELEVANCE, read_num};
use crate::server::text::js_trim;
use crate::server::time::{DAY_MS, EpochMs};

/// 키워드를 다시 만들 때 참고할 기사 기간과 개수(2026-10-21 결정)
pub const HEADLINE_DAYS: i64 = 7;
pub const MAX_HEADLINES: usize = 20;

static WHITESPACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\s\u{FEFF}]+").expect("정규식이 올바르다"));

/// 키워드를 만들 때 LLM에 함께 주는 그 종목의 최근 주요 기사 제목(SPEC FR-3, 2026-10-21 결정).
/// LLM은 오늘 뉴스를 모르므로 이것으로 요즘 이슈를 알려 준다. 종목 이름·Finnhub 직접으로 걸리고
/// 관련도가 표시 기준 이상인 최근 7일 기사를 최근순으로 최대 20개(같은 제목은 하나).
/// 키워드로 걸린 기사는 넣지 않는다(키워드가 스스로를 강화하지 않게).
pub fn recent_headlines(
    conn: &Connection,
    stock_id: i64,
    now: EpochMs,
) -> rusqlite::Result<Vec<String>> {
    let min_relevance = read_num(conn, &EVALUATION_MIN_RELEVANCE)?;
    let mut stmt = conn.prepare(
        "SELECT articles.title FROM article_matches \
         INNER JOIN articles ON articles.id = article_matches.article_id \
         INNER JOIN evaluations ON evaluations.match_id = article_matches.id \
         WHERE article_matches.stock_id = ?1 \
           AND article_matches.match_kind IN ('name', 'direct') \
           AND evaluations.relevance >= ?2 \
           AND articles.published_at >= ?3 \
         ORDER BY articles.published_at DESC, articles.id DESC",
    )?;
    let rows = stmt.query_map(
        params![stock_id, min_relevance, now - HEADLINE_DAYS * DAY_MS],
        |row| row.get::<_, String>(0),
    )?;
    let mut titles = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for row in rows {
        let raw = row?;
        let title = js_trim(&WHITESPACE.replace_all(&raw, " ")).to_string();
        if title.is_empty() || !seen.insert(title.clone()) {
            continue;
        }
        titles.push(title);
        if titles.len() >= MAX_HEADLINES {
            break;
        }
    }
    Ok(titles)
}

#[cfg(test)]
#[path = "headlines_tests.rs"]
mod tests;
