use std::collections::HashSet;

use rusqlite::{Connection, params, params_from_iter};

use crate::server::db::client::atomically;
use crate::server::time::EpochMs;

/// 한 번에 읽음으로 바꿀 수 있는 기사 수(한 카드에 묶이는 기사보다 넉넉하게)
pub const MAX_READ_BATCH: usize = 200;

fn placeholders(count: usize) -> String {
    vec!["?"; count].join(", ")
}

/// 이 기사들을 읽음(read=true) 또는 안 읽음으로 저장한다(12단계).
/// 없는 기사 id는 건너뛰고, 이미 읽은 기사의 읽은 시각은 바꾸지 않는다. 실제로 바뀐 기사 수를 돌려준다.
pub fn set_articles_read(
    conn: &Connection,
    article_ids: &[i64],
    read: bool,
    now: EpochMs,
) -> rusqlite::Result<usize> {
    let mut seen = HashSet::new();
    let ids: Vec<i64> = article_ids
        .iter()
        .copied()
        .filter(|id| seen.insert(*id))
        .collect();
    if ids.is_empty() {
        return Ok(0);
    }
    let marks = placeholders(ids.len());
    atomically(conn, || {
        if !read {
            return conn.execute(
                &format!("DELETE FROM article_reads WHERE article_id IN ({marks})"),
                params_from_iter(ids.iter()),
            );
        }
        let mut stmt = conn.prepare(&format!("SELECT id FROM articles WHERE id IN ({marks})"))?;
        let existing: Vec<i64> = stmt
            .query_map(params_from_iter(ids.iter()), |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let mut changed = 0;
        for id in existing {
            changed += conn.execute(
                "INSERT INTO article_reads (article_id, read_at) VALUES (?1, ?2) ON CONFLICT DO NOTHING",
                params![id, now],
            )?;
        }
        Ok(changed)
    })
}

/// 이 기사들 가운데 읽은 기사의 id
pub fn read_article_ids(conn: &Connection, article_ids: &[i64]) -> rusqlite::Result<HashSet<i64>> {
    if article_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let mut stmt = conn.prepare(&format!(
        "SELECT article_id FROM article_reads WHERE article_id IN ({})",
        placeholders(article_ids.len())
    ))?;
    stmt.query_map(params_from_iter(article_ids.iter()), |row| row.get(0))?
        .collect()
}

#[cfg(test)]
#[path = "reads_tests.rs"]
mod tests;
