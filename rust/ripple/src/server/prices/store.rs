use rusqlite::{Connection, OptionalExtension, params};

use crate::server::db::schema::{Market, PriceSource};
use crate::server::stocks::repository::list_active_stocks;
use crate::server::time::EpochMs;

use super::plan::is_market_open;

/// `stock_prices` 표의 한 줄
#[derive(Debug, Clone, PartialEq)]
pub struct StockPrice {
    pub id: i64,
    pub stock_id: i64,
    pub price: f64,
    pub change: f64,
    pub change_percent: f64,
    pub source: PriceSource,
    pub quoted_at: Option<EpochMs>,
    pub fetched_at: EpochMs,
}

/// 새로 쌓을 주가 한 줄(`id`는 DB가 정한다)
#[derive(Debug, Clone, PartialEq)]
pub struct NewPrice {
    pub stock_id: i64,
    pub price: f64,
    pub change: f64,
    pub change_percent: f64,
    pub source: PriceSource,
    pub quoted_at: Option<EpochMs>,
    pub fetched_at: EpochMs,
}

/// 받은 주가 한 줄을 쌓는다(worker만, 13단계: 덮어쓰지 않고 이력으로).
pub fn save_price(conn: &Connection, row: &NewPrice) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO stock_prices (stock_id, price, change, change_percent, source, quoted_at, fetched_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            row.stock_id,
            row.price,
            row.change,
            row.change_percent,
            row.source.as_str(),
            row.quoted_at,
            row.fetched_at
        ],
    )?;
    Ok(())
}

/// 종목의 가장 최근 주가. 받은 적이 없으면 None
pub fn latest_price(conn: &Connection, stock_id: i64) -> rusqlite::Result<Option<StockPrice>> {
    conn.query_row(
        "SELECT id, stock_id, price, change, change_percent, source, quoted_at, fetched_at \
         FROM stock_prices WHERE stock_id = ?1 ORDER BY fetched_at DESC, id DESC LIMIT 1",
        params![stock_id],
        |row| {
            let source: String = row.get(5)?;
            Ok(StockPrice {
                id: row.get(0)?,
                stock_id: row.get(1)?,
                price: row.get(2)?,
                change: row.get(3)?,
                change_percent: row.get(4)?,
                source: source.parse().map_err(|e: String| {
                    rusqlite::Error::FromSqlConversionFailure(
                        5,
                        rusqlite::types::Type::Text,
                        e.into(),
                    )
                })?,
                quoted_at: row.get(6)?,
                fetched_at: row.get(7)?,
            })
        },
    )
    .optional()
}

#[derive(Debug, Clone, PartialEq)]
pub struct PriceStripItem {
    pub stock_id: i64,
    pub name: String,
    pub market: Market,
    pub price: f64,
    pub change: f64,
    pub change_percent: f64,
    /// 시세 시각(받은 곳이 알려 주지 않으면 받은 시각)
    pub at: EpochMs,
    /// 지금 그 시장이 열려 있는지. 닫혀 있으면 화면에 "장 마감"
    pub open: bool,
}

/// 타임라인 맨 위 주가 띠(13단계). 등록 순서대로, 주가를 받은 적 있는 종목만.
/// (설계상 R11 몫이지만 시험이 `prices/run.test.ts`에 있어 R8로 당겨 옮겼다.)
pub fn read_price_strip(conn: &Connection, now: EpochMs) -> rusqlite::Result<Vec<PriceStripItem>> {
    let mut items = Vec::new();
    for stock in list_active_stocks(conn)? {
        let Some(price) = latest_price(conn, stock.id)? else {
            continue;
        };
        items.push(PriceStripItem {
            stock_id: stock.id,
            name: stock.name_ko.clone().unwrap_or_else(|| stock.name.clone()),
            market: stock.market,
            price: price.price,
            change: price.change,
            change_percent: price.change_percent,
            at: price.quoted_at.unwrap_or(price.fetched_at),
            open: is_market_open(stock.market, now),
        });
    }
    Ok(items)
}
