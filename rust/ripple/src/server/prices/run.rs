use std::collections::BTreeSet;
use std::sync::Arc;

use crate::server::collect::finnhub::FinnhubQuoteClient;
use crate::server::db::client::Db;
use crate::server::db::schema::{Market, PriceSource};
use crate::server::log::{log, log_error};
use crate::server::settings::{PRICES_CHECKED_AT, PRICES_ERROR, PRICES_NOTE, write_num, write_str};
use crate::server::stocks::repository::list_active_stocks;
use crate::server::time::{EpochMs, now_ms};

use super::kis::KisClient;
use super::plan::is_price_window;
use super::store::{NewPrice, latest_price, save_price};

type NowFn = Arc<dyn Fn() -> EpochMs + Send + Sync>;

/// 종목 주가 받기(13단계). 스케줄러가 1분마다 부른다.
/// 장중(plan.rs)이거나 아직 한 번도 받지 못한 종목만 받는다. 한국 종목은 KIS, 미국 종목은 Finnhub(2026-10-30 결정).
/// 한 종목이 실패해도 나머지는 받고, 오류는 상태 화면에 보이게 남긴다.
pub struct PriceRefresher<K, F> {
    db: Db,
    kis: Option<Arc<K>>,
    finnhub: Option<Arc<F>>,
    now: NowFn,
    /// 1분마다 같은 오류를 되풀이해 로그에 남기지 않게 지난 회차의 오류를 기억한다.
    last_error: std::sync::Mutex<String>,
}

struct Quote {
    price: f64,
    change: f64,
    change_percent: f64,
    source: PriceSource,
    quoted_at: Option<EpochMs>,
}

impl<K: KisClient, F: FinnhubQuoteClient> PriceRefresher<K, F> {
    pub fn new(db: Db, kis: Option<Arc<K>>, finnhub: Option<Arc<F>>, now: Option<NowFn>) -> Self {
        Self {
            db,
            kis,
            finnhub,
            now: now.unwrap_or_else(|| Arc::new(now_ms)),
            last_error: std::sync::Mutex::new(String::new()),
        }
    }

    /// 받을 때가 된 종목의 주가를 받아 쌓는다. 오류는 로그와 상태로만 남긴다. 받으려 한 종목이 없으면 false
    pub async fn refresh(&self) -> bool {
        let at = (self.now)();
        let stocks = {
            let conn = self.db.lock().expect("DB 잠금이 깨졌습니다");
            match list_active_stocks(&conn) {
                Ok(stocks) => stocks,
                Err(error) => {
                    log_error("주가 받을 종목을 읽지 못했습니다", &error);
                    return false;
                }
            }
        };
        let mut missing: BTreeSet<&'static str> = BTreeSet::new();
        let mut order: Vec<&'static str> = Vec::new();
        let mut errors: Vec<String> = Vec::new();
        let mut attempted = 0;
        for stock in &stocks {
            if stock.market == Market::Kr && self.kis.is_none() {
                note_missing(
                    &mut missing,
                    &mut order,
                    "KIS 키(KIS_APP_KEY·KIS_APP_SECRET)가 없어 한국 종목",
                );
                continue;
            }
            if stock.market == Market::Us && self.finnhub.is_none() {
                note_missing(&mut missing, &mut order, "FINNHUB_API_KEY가 없어 미국 종목");
                continue;
            }
            if !is_price_window(stock.market, at) {
                let has_price = {
                    let conn = self.db.lock().expect("DB 잠금이 깨졌습니다");
                    latest_price(&conn, stock.id).map(|p| p.is_some())
                };
                // 읽지 못하면 받아 본다(TS는 예외가 나면 이 종목에서 회차 전체가 끝났다).
                if has_price.unwrap_or(false) {
                    continue;
                }
            }
            attempted += 1;
            let name = stock.name_ko.clone().unwrap_or_else(|| stock.name.clone());
            match self.fetch_quote(stock.market, &stock.code).await {
                Ok(Some(quote)) => {
                    let saved = {
                        let conn = self.db.lock().expect("DB 잠금이 깨졌습니다");
                        save_price(
                            &conn,
                            &NewPrice {
                                stock_id: stock.id,
                                price: quote.price,
                                change: quote.change,
                                change_percent: quote.change_percent,
                                source: quote.source,
                                quoted_at: quote.quoted_at,
                                fetched_at: (self.now)(),
                            },
                        )
                    };
                    if let Err(error) = saved {
                        errors.push(format!("{name}: {error}"));
                    }
                }
                Ok(None) => errors.push(format!("{name}: 값이 없습니다.")),
                Err(message) => errors.push(format!("{name}: {message}")),
            }
        }
        let note = if order.is_empty() {
            String::new()
        } else {
            format!("{} 주가는 받지 않습니다.", order.join(", "))
        };
        let state = (|| -> rusqlite::Result<()> {
            let conn = self.db.lock().expect("DB 잠금이 깨졌습니다");
            write_str(&conn, &PRICES_NOTE, &note, at)?;
            if attempted == 0 {
                return Ok(());
            }
            write_num(&conn, &PRICES_CHECKED_AT, at as f64, at)?;
            write_str(&conn, &PRICES_ERROR, &errors.join(" / "), at)
        })();
        if let Err(error) = state {
            log_error("주가 상태 저장 실패", &error);
            return attempted > 0;
        }
        if attempted > 0 {
            let error = errors.join(" / ");
            let mut last = self.last_error.lock().expect("주가 오류 잠금이 깨졌습니다");
            if error != *last {
                if !error.is_empty() {
                    log(&format!("주가 받기 실패: {error}"));
                } else if !last.is_empty() {
                    log("주가 받기 다시 정상");
                }
                *last = error;
            }
        }
        attempted > 0
    }

    async fn fetch_quote(&self, market: Market, code: &str) -> Result<Option<Quote>, String> {
        match market {
            Market::Kr => {
                let Some(kis) = &self.kis else {
                    return Ok(None);
                };
                // KIS 현재가 응답에는 시세 시각이 없어 받은 시각을 쓴다.
                let quote = kis.domestic_quote(code).await.map_err(|e| e.to_string())?;
                Ok(Some(Quote {
                    price: quote.price,
                    change: quote.change,
                    change_percent: quote.change_percent,
                    source: PriceSource::Kis,
                    quoted_at: None,
                }))
            }
            Market::Us => {
                let Some(finnhub) = &self.finnhub else {
                    return Ok(None);
                };
                let quote = finnhub.quote(code).await.map_err(|e| e.to_string())?;
                Ok(quote.map(|q| Quote {
                    price: q.price,
                    change: q.change,
                    change_percent: q.change_percent,
                    source: PriceSource::Finnhub,
                    quoted_at: Some(q.quoted_at),
                }))
            }
        }
    }
}

/// JS `Set`처럼 처음 넣은 순서를 지키며 중복을 뺀다.
fn note_missing(
    seen: &mut BTreeSet<&'static str>,
    order: &mut Vec<&'static str>,
    text: &'static str,
) {
    if seen.insert(text) {
        order.push(text);
    }
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
