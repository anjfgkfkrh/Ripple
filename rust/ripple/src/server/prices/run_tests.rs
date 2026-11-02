use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use crate::server::collect::finnhub::{FinnhubError, FinnhubQuote};
use crate::server::prices::kis::{KisError, KisQuote};
use crate::server::prices::store::{PriceStripItem, read_price_strip};
use crate::server::settings::{read_num, read_str};
use crate::server::stocks::repository::{
    RegisterInput, RegisterResult, delete_stock, register_stock,
};
use crate::test_support::{SharedTestDb, ms, seed_listings, shared_test_db};

/// 한국 시간 "YYYY-MM-DD HH:MM"
fn kst(text: &str) -> EpochMs {
    ms(&format!("{}:00+09:00", text.replace(' ', "T")))
}

struct Seeded {
    shared: SharedTestDb,
    kr_id: i64,
    us_id: i64,
}

/// 한국 종목(삼성전자)과 미국 종목(NVDA)을 가진 시험 DB
fn seed() -> Seeded {
    let shared = shared_test_db();
    let ids = {
        let mut conn = shared.db.lock().unwrap();
        seed_listings(&mut conn);
        [
            (Market::Kr, "005930", "삼성전자", None),
            (Market::Us, "NVDA", "NVDA", Some("엔비디아")),
        ]
        .map(|(market, code, user_input, name_ko)| {
            let result = register_stock(
                &conn,
                &RegisterInput {
                    market,
                    code,
                    user_input,
                    name_ko,
                },
                kst("2026-10-30 09:00"),
            )
            .unwrap();
            match result {
                RegisterResult::Created(stock) => stock.id,
                other => panic!("표본에 없음: {other:?}"),
            }
        })
    };
    Seeded {
        shared,
        kr_id: ids[0],
        us_id: ids[1],
    }
}

#[derive(Default)]
struct FakeKis {
    codes: Mutex<Vec<String>>,
    fail: AtomicBool,
}

impl KisClient for FakeKis {
    async fn domestic_quote(&self, code: &str) -> Result<KisQuote, KisError> {
        self.codes.lock().unwrap().push(code.to_string());
        if self.fail.load(Ordering::SeqCst) {
            return Err(KisError("KIS 시세 오류(HTTP 500)".into()));
        }
        Ok(KisQuote {
            price: 152300.0,
            change: 2300.0,
            change_percent: 1.53,
        })
    }
}

#[derive(Default)]
struct FakeFinnhub {
    symbols: Mutex<Vec<String>>,
}

impl FinnhubQuoteClient for FakeFinnhub {
    async fn quote(&self, symbol: &str) -> Result<Option<FinnhubQuote>, FinnhubError> {
        self.symbols.lock().unwrap().push(symbol.to_string());
        Ok(Some(FinnhubQuote {
            price: 201.5,
            change: -3.25,
            change_percent: -1.59,
            quoted_at: ms("2026-10-29T20:00:00Z"),
        }))
    }
}

type Clock = Arc<std::sync::atomic::AtomicI64>;

fn clock(at: EpochMs) -> Clock {
    Arc::new(std::sync::atomic::AtomicI64::new(at))
}

fn refresher(
    shared: &SharedTestDb,
    kis: Option<Arc<FakeKis>>,
    finnhub: Option<Arc<FakeFinnhub>>,
    clock: &Clock,
) -> PriceRefresher<FakeKis, FakeFinnhub> {
    let clock = clock.clone();
    PriceRefresher::new(
        shared.db.clone(),
        kis,
        finnhub,
        Some(Arc::new(move || clock.load(Ordering::SeqCst))),
    )
}

fn price_count(shared: &SharedTestDb) -> i64 {
    let conn = shared.db.lock().unwrap();
    conn.query_row("SELECT COUNT(*) FROM stock_prices", [], |row| row.get(0))
        .unwrap()
}

// describe("createPriceRefresher (13단계)")
#[tokio::test(flavor = "current_thread")]
async fn receives_only_open_markets_kis_for_korea_and_finnhub_for_us() {
    // it("장중인 시장의 종목만 받고, 한국은 KIS(시세 시각 없음)·미국은 Finnhub로 쌓는다")
    let Seeded { shared, kr_id, .. } = seed();
    let kis = Arc::new(FakeKis::default());
    let finnhub = Arc::new(FakeFinnhub::default());
    let now = clock(kst("2026-10-30 10:00"));
    let refresher = refresher(&shared, Some(kis.clone()), Some(finnhub.clone()), &now);
    // 처음에는 장 밖인 미국 종목도 한 번 받는다(값이 하나도 없으므로).
    assert!(refresher.refresh().await);
    assert_eq!(*kis.codes.lock().unwrap(), ["005930"]);
    assert_eq!(*finnhub.symbols.lock().unwrap(), ["NVDA"]);

    // 1분 뒤: 한국은 장중이라 또 받고, 미국은 장 밖이고 값이 있으니 건너뛴다.
    now.store(kst("2026-10-30 10:01"), Ordering::SeqCst);
    refresher.refresh().await;
    assert_eq!(kis.codes.lock().unwrap().len(), 2);
    assert_eq!(finnhub.symbols.lock().unwrap().len(), 1);

    assert_eq!(price_count(&shared), 3);
    let conn = shared.db.lock().unwrap();
    let (price, change, percent, source, quoted_at, fetched_at): (
        f64,
        f64,
        f64,
        String,
        Option<i64>,
        i64,
    ) = conn
        .query_row(
            "SELECT price, change, change_percent, source, quoted_at, fetched_at FROM stock_prices \
             WHERE stock_id = ?1 ORDER BY id ASC LIMIT 1",
            [kr_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        (
            price,
            change,
            percent,
            source.as_str(),
            quoted_at,
            fetched_at
        ),
        (152300.0, 2300.0, 1.53, "kis", None, kst("2026-10-30 10:00"))
    );
    assert_eq!(
        read_num(&conn, &PRICES_CHECKED_AT).unwrap(),
        kst("2026-10-30 10:01") as f64
    );
    assert_eq!(read_str(&conn, &PRICES_ERROR).unwrap(), "");
    assert_eq!(read_str(&conn, &PRICES_NOTE).unwrap(), "");
}

#[tokio::test(flavor = "current_thread")]
async fn stores_us_quote_time_given_by_finnhub() {
    let Seeded { shared, us_id, .. } = seed();
    let now = clock(kst("2026-10-30 10:00"));
    refresher(
        &shared,
        Some(Arc::new(FakeKis::default())),
        Some(Arc::new(FakeFinnhub::default())),
        &now,
    )
    .refresh()
    .await;
    let conn = shared.db.lock().unwrap();
    let (source, quoted_at): (String, Option<i64>) = conn
        .query_row(
            "SELECT source, quoted_at FROM stock_prices WHERE stock_id = ?1",
            [us_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(source, "finnhub");
    assert_eq!(quoted_at, Some(ms("2026-10-29T20:00:00Z")));
}

#[tokio::test(flavor = "current_thread")]
async fn receives_nothing_when_closed_and_every_stock_has_a_value() {
    // it("장 밖이고 모든 종목에 값이 있으면 아무것도 받지 않는다")
    let Seeded { shared, .. } = seed();
    let kis = Arc::new(FakeKis::default());
    let finnhub = Arc::new(FakeFinnhub::default());
    let now = clock(kst("2026-10-31 10:00")); // 토요일
    let refresher = refresher(&shared, Some(kis.clone()), Some(finnhub.clone()), &now);
    refresher.refresh().await;
    now.store(kst("2026-10-31 10:01"), Ordering::SeqCst);
    assert!(!refresher.refresh().await);
    assert_eq!(kis.codes.lock().unwrap().len(), 1);
    assert_eq!(finnhub.symbols.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn one_failure_does_not_stop_the_rest_and_error_clears_on_success() {
    // it("한 종목이 실패해도 나머지는 받고, 오류를 상태에 남겼다가 다음에 성공하면 지운다")
    let Seeded { shared, .. } = seed();
    let kis = Arc::new(FakeKis::default());
    kis.fail.store(true, Ordering::SeqCst);
    let now = clock(kst("2026-10-30 10:00"));
    let refresher = refresher(
        &shared,
        Some(kis.clone()),
        Some(Arc::new(FakeFinnhub::default())),
        &now,
    );
    refresher.refresh().await;
    {
        let conn = shared.db.lock().unwrap();
        assert_eq!(
            read_str(&conn, &PRICES_ERROR).unwrap(),
            "삼성전자: KIS 시세 오류(HTTP 500)"
        );
    }
    assert_eq!(price_count(&shared), 1);
    kis.fail.store(false, Ordering::SeqCst);
    refresher.refresh().await;
    let conn = shared.db.lock().unwrap();
    assert_eq!(read_str(&conn, &PRICES_ERROR).unwrap(), "");
}

#[tokio::test(flavor = "current_thread")]
async fn joins_several_errors_with_a_slash_in_registration_order() {
    // TS: errors.join(" / ")
    let Seeded { shared, .. } = seed();
    let kis = Arc::new(FakeKis::default());
    kis.fail.store(true, Ordering::SeqCst);
    let now = clock(kst("2026-10-30 22:40"));
    let refresher: PriceRefresher<FakeKis, FailingFinnhub> = PriceRefresher::new(
        shared.db.clone(),
        Some(kis),
        Some(Arc::new(FailingFinnhub)),
        Some(Arc::new({
            let now = now.clone();
            move || now.load(Ordering::SeqCst)
        })),
    );
    refresher.refresh().await;
    let conn = shared.db.lock().unwrap();
    assert_eq!(
        read_str(&conn, &PRICES_ERROR).unwrap(),
        "삼성전자: KIS 시세 오류(HTTP 500) / 엔비디아: Finnhub 호출 한도를 넘었습니다."
    );
}

struct FailingFinnhub;

impl FinnhubQuoteClient for FailingFinnhub {
    async fn quote(&self, _symbol: &str) -> Result<Option<FinnhubQuote>, FinnhubError> {
        Err(FinnhubError("Finnhub 호출 한도를 넘었습니다.".into()))
    }
}

#[tokio::test(flavor = "current_thread")]
async fn empty_finnhub_quote_is_recorded_as_no_value_error() {
    // TS: `if (!quote) throw new Error("값이 없습니다.")`
    struct EmptyFinnhub;
    impl FinnhubQuoteClient for EmptyFinnhub {
        async fn quote(&self, _symbol: &str) -> Result<Option<FinnhubQuote>, FinnhubError> {
            Ok(None)
        }
    }
    let Seeded { shared, .. } = seed();
    let now = clock(kst("2026-10-30 22:40"));
    let refresher: PriceRefresher<FakeKis, EmptyFinnhub> = PriceRefresher::new(
        shared.db.clone(),
        Some(Arc::new(FakeKis::default())),
        Some(Arc::new(EmptyFinnhub)),
        Some(Arc::new({
            let now = now.clone();
            move || now.load(Ordering::SeqCst)
        })),
    );
    refresher.refresh().await;
    let conn = shared.db.lock().unwrap();
    assert_eq!(
        read_str(&conn, &PRICES_ERROR).unwrap(),
        "엔비디아: 값이 없습니다."
    );
}

#[tokio::test(flavor = "current_thread")]
async fn market_without_key_is_skipped_with_a_status_note_and_deleted_stock_is_ignored() {
    // it("키가 없는 시장은 받지 않고 상태 화면용 안내를 남긴다. 지운 종목은 받지 않는다")
    let Seeded { shared, us_id, .. } = seed();
    {
        let conn = shared.db.lock().unwrap();
        delete_stock(&conn, us_id, kst("2026-10-30 09:30")).unwrap();
    }
    let now = clock(kst("2026-10-30 10:00"));
    let refresher = refresher(&shared, None, Some(Arc::new(FakeFinnhub::default())), &now);
    assert!(!refresher.refresh().await);
    let conn = shared.db.lock().unwrap();
    assert_eq!(
        read_str(&conn, &PRICES_NOTE).unwrap(),
        "KIS 키(KIS_APP_KEY·KIS_APP_SECRET)가 없어 한국 종목 주가는 받지 않습니다."
    );
    drop(conn);
    assert_eq!(price_count(&shared), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn notes_both_missing_keys_without_repeating_and_clears_the_note_when_fixed() {
    // TS: Set으로 중복을 빼고 ", "로 이으며, 키가 생기면 안내를 지운다.
    let Seeded { shared, .. } = seed();
    let now = clock(kst("2026-10-30 10:00"));
    let none: PriceRefresher<FakeKis, FakeFinnhub> = refresher(&shared, None, None, &now);
    assert!(!none.refresh().await);
    {
        let conn = shared.db.lock().unwrap();
        assert_eq!(
            read_str(&conn, &PRICES_NOTE).unwrap(),
            "KIS 키(KIS_APP_KEY·KIS_APP_SECRET)가 없어 한국 종목, FINNHUB_API_KEY가 없어 미국 종목 주가는 받지 않습니다."
        );
    }
    let both = refresher(
        &shared,
        Some(Arc::new(FakeKis::default())),
        Some(Arc::new(FakeFinnhub::default())),
        &now,
    );
    assert!(both.refresh().await);
    let conn = shared.db.lock().unwrap();
    assert_eq!(read_str(&conn, &PRICES_NOTE).unwrap(), "");
}

// describe("readPriceStrip (13단계)")
#[test]
fn strip_lists_latest_value_and_open_flag_in_registration_order_skipping_stocks_without_values() {
    // it("등록 순서대로 가장 최근 값과 장이 열렸는지를 돌려주고, 값이 없는 종목은 뺀다")
    let Seeded {
        shared,
        kr_id,
        us_id,
    } = seed();
    let conn = shared.db.lock().unwrap();
    let insert = |stock_id: i64,
                  price: f64,
                  change: f64,
                  source: &str,
                  quoted_at: Option<i64>,
                  fetched_at: i64| {
        save_price(
            &conn,
            &NewPrice {
                stock_id,
                price,
                change,
                change_percent: change,
                source: source.parse().unwrap(),
                quoted_at,
                fetched_at,
            },
        )
        .unwrap();
    };
    insert(kr_id, 100.0, 0.0, "kis", None, kst("2026-10-30 10:00"));
    insert(kr_id, 101.0, 1.0, "kis", None, kst("2026-10-30 10:01"));
    assert_eq!(
        read_price_strip(&conn, kst("2026-10-30 10:02")).unwrap(),
        [PriceStripItem {
            stock_id: kr_id,
            name: "삼성전자".into(),
            market: Market::Kr,
            price: 101.0,
            change: 1.0,
            change_percent: 1.0,
            at: kst("2026-10-30 10:01"),
            open: true,
        }]
    );
    insert(
        us_id,
        201.5,
        0.0,
        "finnhub",
        Some(ms("2026-10-29T20:00:00Z")),
        kst("2026-10-30 10:00"),
    );
    let strip = read_price_strip(&conn, kst("2026-10-30 21:00")).unwrap();
    assert_eq!(
        strip
            .iter()
            .map(|item| (item.name.as_str(), item.open))
            .collect::<Vec<_>>(),
        [("삼성전자", false), ("엔비디아", false)]
    );
    // 미국은 Finnhub가 알려 준 시세 시각
    assert_eq!(strip[1].at, ms("2026-10-29T20:00:00Z"));
}

#[test]
fn latest_price_breaks_ties_on_fetched_at_by_id() {
    // TS: orderBy(desc(fetchedAt), desc(id))
    let Seeded { shared, kr_id, .. } = seed();
    let conn = shared.db.lock().unwrap();
    for price in [1.0, 2.0] {
        save_price(
            &conn,
            &NewPrice {
                stock_id: kr_id,
                price,
                change: 0.0,
                change_percent: 0.0,
                source: PriceSource::Kis,
                quoted_at: None,
                fetched_at: kst("2026-10-30 10:00"),
            },
        )
        .unwrap();
    }
    assert_eq!(latest_price(&conn, kr_id).unwrap().unwrap().price, 2.0);
    assert_eq!(latest_price(&conn, kr_id + 99).unwrap(), None);
}
