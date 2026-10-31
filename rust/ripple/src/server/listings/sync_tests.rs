use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::server::listings::lookup::find_listing;
use crate::server::settings::LISTINGS_SYNCED_AT_KR as SYNCED_KR;
use crate::test_support::{FIXTURE_FILES, fixture_bytes, ms, shared_test_db, test_db};

/// 표본 파일을 돌려주는 가짜 서버. `failing`에 든 경로는 503을 준다.
async fn fixture_server(failing: &[&str]) -> MockServer {
    let server = MockServer::start().await;
    for (route, file) in [
        ("/kind", FIXTURE_FILES.kind),
        ("/nasdaqlisted", FIXTURE_FILES.nasdaq_listed),
        ("/otherlisted", FIXTURE_FILES.other_listed),
    ] {
        let response = if failing.contains(&route) {
            ResponseTemplate::new(503).set_body_string("down")
        } else {
            ResponseTemplate::new(200).set_body_bytes(fixture_bytes(file))
        };
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(response)
            .mount(&server)
            .await;
    }
    server
}

fn options<'a>(
    server: &MockServer,
    now: Option<&'a (dyn Fn() -> EpochMs + Send + Sync)>,
) -> SyncOptions<'a> {
    SyncOptions {
        sources: ListingSources {
            kind: format!("{}/kind", server.uri()),
            nasdaq_listed: format!("{}/nasdaqlisted", server.uri()),
            other_listed: format!("{}/otherlisted", server.uri()),
        },
        http: reqwest::Client::new(),
        now,
    }
}

// describe("공식 종목 목록 동기화 (SPEC D-12)")
#[tokio::test(flavor = "current_thread")]
async fn downloads_kind_and_two_nasdaq_files_saves_them_and_records_time() {
    // it("한국은 KIND(EUC-KR), 미국은 NASDAQ 두 파일을 받아 저장하고 시각을 남긴다")
    let shared = shared_test_db();
    let server = fixture_server(&[]).await;
    let now = ms("2026-10-20T00:00:00Z");
    let at = || now;

    let results = sync_listings_if_stale(&shared.db, &options(&server, Some(&at)))
        .await
        .unwrap();

    assert_eq!(
        results,
        [
            MarketSyncResult::Ok {
                market: Market::Kr,
                count: 7
            },
            MarketSyncResult::Ok {
                market: Market::Us,
                count: 9
            },
        ]
    );
    {
        let conn = shared.db.lock().unwrap();
        assert_eq!(
            find_listing(&conn, Market::Kr, "005930")
                .unwrap()
                .unwrap()
                .name,
            "삼성전자"
        );
        assert_eq!(
            find_listing(&conn, Market::Us, "NVDA")
                .unwrap()
                .unwrap()
                .exchange,
            "NASDAQ"
        );
        assert_eq!(read_num(&conn, &SYNCED_KR).unwrap(), now as f64);
    }
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests.iter().all(|r| {
        r.headers
            .get("user-agent")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("Ripple/")
    }));
}

#[tokio::test(flavor = "current_thread")]
async fn does_not_download_again_within_seven_days() {
    // it("받은 지 7일이 안 됐으면 다시 받지 않는다")
    let shared = shared_test_db();
    let server = fixture_server(&[]).await;
    let start = ms("2026-10-20T00:00:00Z");
    let at_start = || start;
    sync_listings_if_stale(&shared.db, &options(&server, Some(&at_start)))
        .await
        .unwrap();
    let before = server.received_requests().await.unwrap().len();

    let almost = || start + LISTINGS_MAX_AGE_MS - 1;
    let results = sync_listings_if_stale(&shared.db, &options(&server, Some(&almost)))
        .await
        .unwrap();
    assert_eq!(results, []);
    assert_eq!(server.received_requests().await.unwrap().len(), before);
}

#[tokio::test(flavor = "current_thread")]
async fn one_market_failing_keeps_its_old_list_and_other_market_updates() {
    // it("한 시장이 실패해도 다른 시장은 갱신하고, 실패한 시장은 이전 목록을 유지한다")
    let shared = shared_test_db();
    let healthy = fixture_server(&[]).await;
    let first = sync_market(&shared.db, Market::Kr, &options(&healthy, None)).await;
    assert!(matches!(first, MarketSyncResult::Ok { .. }));

    let broken = fixture_server(&["/kind"]).await;
    let later = || now_ms() + LISTINGS_MAX_AGE_MS;
    let results = sync_listings_if_stale(&shared.db, &options(&broken, Some(&later)))
        .await
        .unwrap();

    assert!(
        matches!(
            &results[0],
            MarketSyncResult::Err {
                market: Market::Kr,
                ..
            }
        ),
        "{results:?}"
    );
    assert!(
        matches!(
            &results[1],
            MarketSyncResult::Ok {
                market: Market::Us,
                ..
            }
        ),
        "{results:?}"
    );
    assert_eq!(
        listing_count(&shared.db.lock().unwrap(), Market::Kr).unwrap(),
        7
    );
}

fn row(code: &str) -> ListingRow {
    ListingRow {
        market: Market::Kr,
        code: code.into(),
        name: code.into(),
        exchange: "KOSPI".into(),
        is_etf: false,
    }
}

#[test]
fn rejects_new_list_smaller_than_half_of_previous() {
    // it("새 목록이 이전의 절반보다 적으면 형식이 바뀐 것으로 보고 거부한다")
    let mut db = test_db();
    let four: Vec<ListingRow> = ["000001", "000002", "000003", "000004"]
        .into_iter()
        .map(row)
        .collect();
    replace_listings(&mut db.conn, Market::Kr, &four).unwrap();

    let error = replace_listings(&mut db.conn, Market::Kr, &[row("000005")]).unwrap_err();
    assert!(error.to_string().contains("비정상적으로 적습니다"));
    assert_eq!(
        error.to_string(),
        "KR 종목 목록이 비정상적으로 적습니다(새 1개, 이전 4개)."
    );
    assert!(replace_listings(&mut db.conn, Market::Kr, &[]).is_err());
    assert_eq!(listing_count(&db.conn, Market::Kr).unwrap(), 4);
}

#[test]
fn duplicate_codes_keep_last_value_in_first_position() {
    // Rust에서만 생기는 경우: JS `new Map(rows.map(...))`의 동작
    let mut db = test_db();
    let mut renamed = row("000001");
    renamed.name = "나중 이름".into();
    replace_listings(
        &mut db.conn,
        Market::Kr,
        &[row("000001"), row("000002"), renamed],
    )
    .unwrap();
    let names: Vec<String> = db
        .conn
        .prepare("SELECT name FROM listings ORDER BY rowid")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(names, ["나중 이름", "000002"]);
}

#[test]
fn exactly_half_of_previous_is_accepted() {
    // 경계: `rows.length < previous / 2`만 거부한다.
    let mut db = test_db();
    let four: Vec<ListingRow> = ["000001", "000002", "000003", "000004"]
        .into_iter()
        .map(row)
        .collect();
    replace_listings(&mut db.conn, Market::Kr, &four).unwrap();
    replace_listings(&mut db.conn, Market::Kr, &[row("000005"), row("000006")]).unwrap();
    assert_eq!(listing_count(&db.conn, Market::Kr).unwrap(), 2);
}
