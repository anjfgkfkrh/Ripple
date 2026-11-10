use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::server::db::schema::{ItemKind, KeywordStatus, Market};
use crate::server::listings::lookup::find_listing;
use crate::server::text::{js_slice, js_trim};
use crate::server::time::EpochMs;

/// `stocks` 표의 한 줄(종목·시장·주제 항목 모두)
#[derive(Debug, Clone, PartialEq)]
pub struct Stock {
    pub id: i64,
    pub market: Market,
    pub code: String,
    pub name: String,
    pub name_ko: Option<String>,
    pub exchange: String,
    pub user_input: String,
    pub created_at: EpochMs,
    pub deleted_at: Option<EpochMs>,
    pub keywords_status: KeywordStatus,
    pub keywords_error: Option<String>,
    pub keywords_generated_at: Option<EpochMs>,
    pub backfill_from: Option<EpochMs>,
    pub kind: ItemKind,
    pub topic_id: Option<i64>,
    /// 회사 별칭(LLM 제안 중 검증을 통과한 것)
    pub aliases: Vec<String>,
    /// 고유어(이 글자가 있어야 연결을 평가한다)
    pub anchors: Vec<String>,
    /// 사용자가 지운 별칭(다시 제안돼도 버린다)
    pub aliases_removed: Vec<String>,
}

pub const STOCK_COLUMNS: &str = "id, market, code, name, name_ko, exchange, user_input, created_at, deleted_at, \
     keywords_status, keywords_error, keywords_generated_at, backfill_from, kind, topic_id, aliases, anchors, aliases_removed";

fn text_enum<T: std::str::FromStr<Err = String>>(
    row: &Row<'_>,
    index: usize,
) -> rusqlite::Result<T> {
    let text: String = row.get(index)?;
    text.parse().map_err(|e: String| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, e.into())
    })
}

pub fn row_to_stock(row: &Row<'_>) -> rusqlite::Result<Stock> {
    Ok(Stock {
        id: row.get(0)?,
        market: text_enum(row, 1)?,
        code: row.get(2)?,
        name: row.get(3)?,
        name_ko: row.get(4)?,
        exchange: row.get(5)?,
        user_input: row.get(6)?,
        created_at: row.get(7)?,
        deleted_at: row.get(8)?,
        keywords_status: text_enum(row, 9)?,
        keywords_error: row.get(10)?,
        keywords_generated_at: row.get(11)?,
        backfill_from: row.get(12)?,
        kind: text_enum(row, 13)?,
        topic_id: row.get(14)?,
        aliases: json_list(row, 15)?,
        anchors: json_list(row, 16)?,
        aliases_removed: json_list(row, 17)?,
    })
}

/// JSON 글자 목록 칸을 읽는다. 깨졌으면 빈 목록(`topics.terms`와 같은 방식).
fn json_list(row: &Row<'_>, index: usize) -> rusqlite::Result<Vec<String>> {
    let text: String = row.get(index)?;
    Ok(serde_json::from_str(&text).unwrap_or_default())
}

/// 별칭·고유어 두 목록을 바꾼다. 없는 종목이면 false.
pub fn set_stock_terms(
    conn: &Connection,
    stock_id: i64,
    aliases: &[String],
    anchors: &[String],
) -> rusqlite::Result<bool> {
    let changed = conn.execute(
        "UPDATE stocks SET aliases = ?1, anchors = ?2 WHERE id = ?3",
        params![
            serde_json::to_string(aliases).expect("string list"),
            serde_json::to_string(anchors).expect("string list"),
            stock_id
        ],
    )?;
    Ok(changed > 0)
}

/// 별칭을 목록에서 빼고 `aliases_removed`에 더한다. 목록에 없던 별칭이면 false.
pub fn remove_alias(conn: &Connection, stock_id: i64, alias: &str) -> rusqlite::Result<bool> {
    let Some(stock) = find_stock(conn, stock_id)? else {
        return Ok(false);
    };
    if !stock.aliases.iter().any(|a| a == alias) {
        return Ok(false);
    }
    let aliases: Vec<&String> = stock.aliases.iter().filter(|a| *a != alias).collect();
    let mut removed = stock.aliases_removed.clone();
    if !removed.iter().any(|a| a == alias) {
        removed.push(alias.to_string());
    }
    conn.execute(
        "UPDATE stocks SET aliases = ?1, aliases_removed = ?2 WHERE id = ?3",
        params![
            serde_json::to_string(&aliases).expect("string list"),
            serde_json::to_string(&removed).expect("string list"),
            stock_id
        ],
    )?;
    Ok(true)
}

pub const MAX_NAME_KO_LENGTH: usize = 50;

/// 삭제하지 않은 종목(시장·주제 항목은 빼고, 11단계), 등록한 순서대로
pub fn list_active_stocks(conn: &Connection) -> rusqlite::Result<Vec<Stock>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {STOCK_COLUMNS} FROM stocks WHERE deleted_at IS NULL AND kind = 'stock' \
         ORDER BY created_at ASC, id ASC"
    ))?;
    stmt.query_map([], row_to_stock)?.collect()
}

pub fn is_active_stock(conn: &Connection, market: Market, code: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM stocks WHERE market = ?1 AND code = ?2 AND deleted_at IS NULL)",
        params![market.as_str(), code],
        |row| row.get(0),
    )
}

pub fn is_active_stock_id(conn: &Connection, id: i64) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM stocks WHERE id = ?1 AND deleted_at IS NULL)",
        params![id],
        |row| row.get(0),
    )
}

pub struct RegisterInput<'a> {
    pub market: Market,
    pub code: &'a str,
    pub user_input: &'a str,
    pub name_ko: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RegisterResult {
    Created(Stock),
    Restored(Stock),
    Exists(Stock),
    NotListed,
}

/// 새로 등록하거나 되살린 종목은 다음 수집 회차에서 이만큼 지난 기사까지 연결한다(2026-10-21 결정).
pub const NEW_STOCK_BACKFILL_MS: i64 = 24 * 60 * 60 * 1000;

/// id로 한 줄을 읽는다(없으면 None). 지운 줄도 읽는다.
pub fn find_stock(conn: &Connection, id: i64) -> rusqlite::Result<Option<Stock>> {
    conn.query_row(
        &format!("SELECT {STOCK_COLUMNS} FROM stocks WHERE id = ?1"),
        params![id],
        row_to_stock,
    )
    .optional()
}

fn select_stock(conn: &Connection, id: i64) -> rusqlite::Result<Stock> {
    conn.query_row(
        &format!("SELECT {STOCK_COLUMNS} FROM stocks WHERE id = ?1"),
        params![id],
        row_to_stock,
    )
}

/// 공식 목록에 있는 종목만 등록한다(D-12). 이름·거래소는 목록 값을 쓴다.
/// 예전에 지운 종목이면 새로 만들지 않고 되살린다(D-11).
pub fn register_stock(
    conn: &Connection,
    input: &RegisterInput<'_>,
    now: EpochMs,
) -> rusqlite::Result<RegisterResult> {
    let Some(listing) = find_listing(conn, input.market, input.code)? else {
        return Ok(RegisterResult::NotListed);
    };

    let name_ko: Option<String> = if listing.market == Market::Kr {
        Some(listing.name.clone())
    } else {
        input
            .name_ko
            .map(|n| js_slice(js_trim(n), 0, MAX_NAME_KO_LENGTH))
            .filter(|n| !n.is_empty())
    };
    let user_input = js_trim(input.user_input);
    let backfill_from = now - NEW_STOCK_BACKFILL_MS;

    let existing = conn
        .query_row(
            &format!("SELECT {STOCK_COLUMNS} FROM stocks WHERE market = ?1 AND code = ?2"),
            params![listing.market.as_str(), listing.code],
            row_to_stock,
        )
        .optional()?;
    if let Some(existing) = existing {
        if existing.deleted_at.is_none() {
            return Ok(RegisterResult::Exists(existing));
        }
        conn.execute(
            "UPDATE stocks SET name = ?1, name_ko = ?2, exchange = ?3, user_input = ?4, backfill_from = ?5, \
             deleted_at = NULL WHERE id = ?6",
            params![listing.name, name_ko, listing.exchange, user_input, backfill_from, existing.id],
        )?;
        return Ok(RegisterResult::Restored(select_stock(conn, existing.id)?));
    }
    conn.execute(
        "INSERT INTO stocks (market, code, name, name_ko, exchange, user_input, backfill_from, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            listing.market.as_str(),
            listing.code,
            listing.name,
            name_ko,
            listing.exchange,
            user_input,
            backfill_from,
            now
        ],
    )?;
    Ok(RegisterResult::Created(select_stock(
        conn,
        conn.last_insert_rowid(),
    )?))
}

/// 지우지 않고 숨긴다(D-11). 이미 숨겼거나 없으면 false
pub fn delete_stock(conn: &Connection, id: i64, now: EpochMs) -> rusqlite::Result<bool> {
    let changes = conn.execute(
        "UPDATE stocks SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
        params![now, id],
    )?;
    Ok(changes > 0)
}

#[cfg(test)]
#[path = "repository_tests.rs"]
mod tests;
