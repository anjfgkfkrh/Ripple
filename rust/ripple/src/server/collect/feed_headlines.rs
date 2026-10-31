use std::sync::LazyLock;

use regex::Regex;
use rusqlite::{Connection, Row, params};

use crate::server::collect::published::effective_published_at;
use crate::server::collect::url::url_key;
use crate::server::text::{js_slice, js_trim};
use crate::server::time::EpochMs;

const HOUR_MS: i64 = 60 * 60 * 1000;
/// 제목을 두는 기간(10단계 결정: 3일)
pub const HEADLINE_KEEP_MS: i64 = 3 * 24 * HOUR_MS;
const TITLE_MAX_CHARS: usize = 300;
const SUMMARY_MAX_CHARS: usize = 300;

static WHITESPACE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]+")
        .expect("정규식이 올바르다")
});

#[derive(Debug, Clone)]
pub struct HeadlineItem {
    pub title: String,
    pub publisher: String,
    pub published_at: Option<EpochMs>,
    /// 원문 주소와 요약(11단계 결정: 새 토픽에 지난 기사를 걸 때 쓴다)
    pub url: Option<String>,
    pub summary: Option<String>,
}

/// 같은 제목인지 볼 때 쓰는 열쇠: 공백을 모두 빼고 소문자로
pub fn headline_key(title: &str) -> String {
    WHITESPACE.replace_all(title, "").to_lowercase()
}

fn collapse(text: &str, max: usize) -> String {
    js_slice(js_trim(&WHITESPACE.replace_all(text, " ")), 0, max)
}

/// 피드에서 받은 기사를 종목 연결과 상관없이 남긴다(토픽 갱신의 재료이자 지난 기사 걸기에 씀, 10·11단계).
/// 제목·언론사·피드·시각과 원문 주소·요약(앞 300자)을 둔다. 같은 제목은 처음 받은 것 하나만,
/// 보관 기간보다 오래된 기사는 넣지 않는다. 새로 넣은 수를 돌려준다.
pub fn save_feed_headlines(
    conn: &mut Connection,
    feed_id: &str,
    items: &[HeadlineItem],
    seen_at: EpochMs,
) -> rusqlite::Result<usize> {
    let oldest = seen_at - HEADLINE_KEEP_MS;
    struct Row2 {
        title_key: String,
        title: String,
        publisher: String,
        published_at: EpochMs,
        url: Option<String>,
        url_key: Option<String>,
        summary: Option<String>,
    }
    let rows: Vec<Row2> = items
        .iter()
        .filter_map(|item| {
            let title = collapse(&item.title, TITLE_MAX_CHARS);
            let published_at = effective_published_at(item.published_at, seen_at);
            if title.is_empty() || published_at < oldest {
                return None;
            }
            let key = item
                .url
                .as_deref()
                .filter(|u| !u.is_empty())
                .and_then(url_key);
            let summary = item
                .summary
                .as_deref()
                .map_or_else(String::new, |s| collapse(s, SUMMARY_MAX_CHARS));
            Some(Row2 {
                title_key: headline_key(&title),
                title,
                publisher: item.publisher.clone(),
                published_at,
                url: match (&key, &item.url) {
                    (Some(_), Some(url)) => Some(js_trim(url).to_string()),
                    _ => None,
                },
                url_key: key,
                summary: Some(summary).filter(|s| !s.is_empty()),
            })
        })
        .collect();
    if rows.is_empty() {
        return Ok(0);
    }
    let tx = conn.transaction()?;
    let mut added = 0;
    {
        let mut insert = tx.prepare(
            "INSERT INTO feed_headlines (title_key, title, publisher, feed, published_at, seen_at, url, url_key, summary) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) ON CONFLICT DO NOTHING",
        )?;
        for row in &rows {
            added += insert.execute(params![
                row.title_key,
                row.title,
                row.publisher,
                feed_id,
                row.published_at,
                seen_at,
                row.url,
                row.url_key,
                row.summary
            ])?;
        }
    }
    tx.commit()?;
    Ok(added)
}

/// 보관 기간이 지난 제목을 지운다. 지운 수를 돌려준다.
pub fn prune_feed_headlines(conn: &Connection, now: EpochMs) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM feed_headlines WHERE published_at < ?1",
        params![now - HEADLINE_KEEP_MS],
    )
}

/// since 이후 제목을 최근순으로 limit개까지
pub fn recent_feed_titles(
    conn: &Connection,
    since: EpochMs,
    limit: i64,
) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT title FROM feed_headlines WHERE published_at >= ?1 ORDER BY published_at DESC, id DESC LIMIT ?2",
    )?;
    stmt.query_map(params![since, limit], |row| row.get(0))?
        .collect()
}

/// `feed_headlines` 표의 한 줄
#[derive(Debug, Clone, PartialEq)]
pub struct StoredHeadline {
    pub id: i64,
    pub title_key: String,
    pub title: String,
    pub publisher: String,
    pub feed: String,
    pub published_at: EpochMs,
    pub seen_at: EpochMs,
    pub url: Option<String>,
    pub url_key: Option<String>,
    pub summary: Option<String>,
}

fn row_to_headline(row: &Row<'_>) -> rusqlite::Result<StoredHeadline> {
    Ok(StoredHeadline {
        id: row.get(0)?,
        title_key: row.get(1)?,
        title: row.get(2)?,
        publisher: row.get(3)?,
        feed: row.get(4)?,
        published_at: row.get(5)?,
        seen_at: row.get(6)?,
        url: row.get(7)?,
        url_key: row.get(8)?,
        summary: row.get(9)?,
    })
}

/// since 이후 제목 가운데 원문 주소가 있는 것(지난 기사를 다시 걸 때, 11단계)
pub fn stored_headlines_since(
    conn: &Connection,
    since: EpochMs,
) -> rusqlite::Result<Vec<StoredHeadline>> {
    let mut stmt = conn.prepare(
        "SELECT id, title_key, title, publisher, feed, published_at, seen_at, url, url_key, summary FROM feed_headlines \
         WHERE published_at >= ?1 AND url_key IS NOT NULL ORDER BY published_at DESC, id DESC",
    )?;
    stmt.query_map(params![since], row_to_headline)?.collect()
}

#[cfg(test)]
#[path = "feed_headlines_tests.rs"]
mod tests;
