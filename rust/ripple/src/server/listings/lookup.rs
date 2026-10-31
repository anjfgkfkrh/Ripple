use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::server::db::schema::Market;
use crate::server::listings::parse::KR_CODE;
use crate::server::text::js_trim;

/// `listings` 표의 한 줄
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub market: Market,
    pub code: String,
    pub name: String,
    pub exchange: String,
    pub is_etf: bool,
}

fn row_to_listing(row: &Row<'_>) -> rusqlite::Result<Listing> {
    let market: String = row.get(0)?;
    Ok(Listing {
        market: market.parse().map_err(|e: String| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
        })?,
        code: row.get(1)?,
        name: row.get(2)?,
        exchange: row.get(3)?,
        is_etf: row.get(4)?,
    })
}

/// LLM이 준 코드를 목록 형식에 맞춘다.
/// - 한국: "A005930"처럼 앞에 붙는 A를 떼고, 숫자만 짧게 오면 앞을 0으로 채운다.
/// - 미국: 클래스 주식 구분자는 목록 형식인 점으로 맞춘다(BRK-B, BRK/B → BRK.B).
pub fn normalize_code(market: Market, code: &str) -> String {
    let upper = js_trim(code).to_uppercase();
    if market == Market::Us {
        return upper.replace(['-', '/', ' '], ".");
    }
    let compact: String = upper
        .chars()
        .filter(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
        .collect();
    // /^A[0-9A-Z]{6}$/ — 문자는 모두 ASCII라 바이트 위치가 곧 글자 위치다.
    if compact.len() == 7 && compact.starts_with('A') {
        return compact[1..].to_string();
    }
    if (1..=5).contains(&compact.len()) && compact.bytes().all(|b| b.is_ascii_digit()) {
        return format!("{compact:0>6}");
    }
    compact
}

pub fn find_listing(
    conn: &Connection,
    market: Market,
    code: &str,
) -> rusqlite::Result<Option<Listing>> {
    let normalized = normalize_code(market, code);
    if market == Market::Kr && !KR_CODE.is_match(&normalized) {
        return Ok(None);
    }
    conn.query_row(
        "SELECT market, code, name, exchange, is_etf FROM listings WHERE market = ?1 AND code = ?2",
        params![market.as_str(), normalized],
        row_to_listing,
    )
    .optional()
}

pub fn find_listing_by_name(
    conn: &Connection,
    market: Market,
    name: &str,
) -> rusqlite::Result<Option<Listing>> {
    conn.query_row(
        "SELECT market, code, name, exchange, is_etf FROM listings WHERE market = ?1 AND name = ?2",
        params![market.as_str(), js_trim(name)],
        row_to_listing,
    )
    .optional()
}

#[cfg(test)]
#[path = "lookup_tests.rs"]
mod tests;
