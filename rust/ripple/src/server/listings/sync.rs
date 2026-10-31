use std::collections::HashMap;
use std::time::Duration;

use rusqlite::{Connection, params};

use crate::server::db::client::Db;
use crate::server::db::schema::Market;
use crate::server::listings::parse::{
    ListingRow, parse_kind_list, parse_nasdaq_listed, parse_other_listed,
};
use crate::server::settings::{
    LISTINGS_SYNCED_AT_KR, LISTINGS_SYNCED_AT_US, NumSetting, read_num, write_num,
};
use crate::server::time::{EpochMs, now_ms};

/// 공식 종목 목록 원천 (D-12). 둘 다 무료이고 키가 필요 없다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingSources {
    pub kind: String,
    pub nasdaq_listed: String,
    pub other_listed: String,
}

pub fn listing_sources() -> ListingSources {
    ListingSources {
        kind: "https://kind.krx.co.kr/corpgeneral/corpList.do?method=download&searchType=13"
            .to_string(),
        nasdaq_listed: "https://www.nasdaqtrader.com/dynamic/SymDir/nasdaqlisted.txt".to_string(),
        other_listed: "https://www.nasdaqtrader.com/dynamic/SymDir/otherlisted.txt".to_string(),
    }
}

/// 주 1회 갱신 (D-12)
pub const LISTINGS_MAX_AGE_MS: i64 = 7 * 24 * 60 * 60 * 1000;

const USER_AGENT: &str = "Ripple/0.1 (personal stock news reader)";
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60);

pub struct SyncOptions<'a> {
    pub sources: ListingSources,
    pub http: reqwest::Client,
    pub now: Option<&'a (dyn Fn() -> EpochMs + Send + Sync)>,
}

impl SyncOptions<'_> {
    fn now(&self) -> EpochMs {
        self.now.map_or_else(now_ms, |f| f())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketSyncResult {
    Ok { market: Market, count: usize },
    Err { market: Market, error: String },
}

fn sync_key(market: Market) -> &'static NumSetting {
    match market {
        Market::Kr => &LISTINGS_SYNCED_AT_KR,
        Market::Us => &LISTINGS_SYNCED_AT_US,
    }
}

/// 이 시장의 목록이 없거나 오래됐으면 받는다. 시장마다 따로 성공·실패한다.
pub async fn sync_listings_if_stale(
    db: &Db,
    options: &SyncOptions<'_>,
) -> rusqlite::Result<Vec<MarketSyncResult>> {
    let now = options.now();
    let mut stale = Vec::new();
    {
        let conn = db.lock().expect("DB 잠금이 깨졌습니다");
        for market in [Market::Kr, Market::Us] {
            let synced = read_num(&conn, sync_key(market))?;
            if now as f64 - synced >= LISTINGS_MAX_AGE_MS as f64 {
                stale.push(market);
            }
        }
    }
    let mut results = Vec::new();
    for market in stale {
        results.push(sync_market(db, market, options).await);
    }
    Ok(results)
}

pub async fn sync_market(db: &Db, market: Market, options: &SyncOptions<'_>) -> MarketSyncResult {
    match try_sync_market(db, market, options).await {
        Ok(count) => MarketSyncResult::Ok { market, count },
        Err(error) => MarketSyncResult::Err { market, error },
    }
}

async fn try_sync_market(
    db: &Db,
    market: Market,
    options: &SyncOptions<'_>,
) -> Result<usize, String> {
    let sources = &options.sources;
    let rows = match market {
        Market::Kr => {
            let bytes = download(&options.http, &sources.kind).await?;
            // TextDecoder("euc-kr")는 BOM을 보지 않는다.
            let (text, _) = encoding_rs::EUC_KR.decode_without_bom_handling(&bytes);
            parse_kind_list(&text).map_err(|e| e.to_string())?
        }
        Market::Us => {
            let nasdaq = download(&options.http, &sources.nasdaq_listed).await?;
            let mut rows = parse_nasdaq_listed(&decode_utf8(&nasdaq)).map_err(|e| e.to_string())?;
            let other = download(&options.http, &sources.other_listed).await?;
            rows.extend(parse_other_listed(&decode_utf8(&other)).map_err(|e| e.to_string())?);
            rows
        }
    };
    let mut conn = db.lock().expect("DB 잠금이 깨졌습니다");
    replace_listings(&mut conn, market, &rows).map_err(|e| e.to_string())?;
    write_num(&conn, sync_key(market), options.now() as f64, options.now())
        .map_err(|e| e.to_string())?;
    Ok(rows.len())
}

/// `new TextDecoder()`: UTF-8이고 앞의 BOM은 뗀다. 깨진 바이트는 대체 문자로 바뀐다.
fn decode_utf8(bytes: &[u8]) -> String {
    encoding_rs::UTF_8
        .decode_with_bom_removal(bytes)
        .0
        .into_owned()
}

#[derive(Debug, thiserror::Error)]
pub enum ReplaceListingsError {
    #[error("{market} 종목 목록이 비정상적으로 적습니다(새 {new}개, 이전 {previous}개).")]
    TooFew {
        market: &'static str,
        new: usize,
        previous: i64,
    },
    #[error("{0}")]
    Db(#[from] rusqlite::Error),
}

/// 시장 단위로 통째로 갈아 끼운다(한 트랜잭션이라 읽는 쪽은 이전 목록 또는 새 목록만 본다).
/// 형식이 바뀌어 일부만 읽혔을 때 목록을 망가뜨리지 않도록, 이전의 절반보다 적으면 거부한다.
pub fn replace_listings(
    conn: &mut Connection,
    market: Market,
    rows: &[ListingRow],
) -> Result<(), ReplaceListingsError> {
    let previous = listing_count(conn, market)?;
    if rows.is_empty() || (rows.len() as f64) < previous as f64 / 2.0 {
        return Err(ReplaceListingsError::TooFew {
            market: market.as_str(),
            new: rows.len(),
            previous,
        });
    }
    // 같은 코드는 뒤의 값을 쓰되 처음 나온 자리를 지킨다(JS `new Map(...)`).
    let mut position: HashMap<&str, usize> = HashMap::new();
    let mut unique: Vec<&ListingRow> = Vec::new();
    for row in rows {
        match position.get(row.code.as_str()) {
            Some(&i) => unique[i] = row,
            None => {
                position.insert(&row.code, unique.len());
                unique.push(row);
            }
        }
    }
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM listings WHERE market = ?1",
        params![market.as_str()],
    )?;
    {
        let mut insert = tx.prepare(
            "INSERT INTO listings (market, code, name, exchange, is_etf) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for row in unique {
            insert.execute(params![
                row.market.as_str(),
                row.code,
                row.name,
                row.exchange,
                row.is_etf
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn listing_count(conn: &Connection, market: Market) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT count(*) FROM listings WHERE market = ?1",
        params![market.as_str()],
        |row| row.get(0),
    )
}

async fn download(http: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let response = http
        .get(url)
        .header("User-Agent", USER_AGENT)
        .timeout(DOWNLOAD_TIMEOUT)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("{url} 응답 {}", response.status().as_u16()));
    }
    response
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
