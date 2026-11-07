use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;

use super::*;
use crate::server::collect::finnhub::FinnhubError;
use crate::server::collect::http::{HttpError, HttpErrorKind, HttpResponse};
use crate::server::collect::runner::CollectionRunner;
use crate::server::db::schema::Market;
use crate::server::keywords::generate::{replace_keywords, set_keyword_status};
use crate::server::keywords::tree::KeywordNode;
use crate::server::stocks::repository::{
    RegisterInput, RegisterResult, delete_stock, register_stock,
};
use crate::test_support::{
    FakeDecider, SharedTestDb, fake_article_decider, fake_decision, ms, seed_listings,
    shared_test_db,
};

// 한국 시간 2026-10-20 12:00
fn now() -> EpochMs {
    ms("2026-10-20T03:00:00Z")
}

const HOUR: i64 = 60 * 60 * 1000;

fn ago(hours: f64) -> EpochMs {
    now() - (hours * HOUR as f64) as i64
}

fn seeded() -> SharedTestDb {
    let shared = shared_test_db();
    seed_listings(&mut shared.db.lock().unwrap());
    shared
}

struct Ids {
    samsung: i64,
    nvidia: i64,
}

fn node(
    key: i64,
    parent_key: Option<i64>,
    level: i64,
    text_ko: &str,
    text_en: Option<&str>,
    path: &str,
) -> KeywordNode {
    KeywordNode {
        key,
        parent_key,
        level,
        kind: crate::server::db::schema::KeywordKind::Structure,
        text_ko: text_ko.into(),
        text_en: text_en.map(str::to_string),
        reason: "r".into(),
        path: path.into(),
        relevance: None,
        magnitude: None,
    }
}

fn add_stocks_at(shared: &SharedTestDb, at: EpochMs) -> Ids {
    let mut conn = shared.db.lock().unwrap();
    let register =
        |market: Market, code: &str, user_input: &str, name_ko: Option<&str>| match register_stock(
            &conn,
            &RegisterInput {
                market,
                code,
                user_input,
                name_ko,
            },
            at,
        )
        .unwrap()
        {
            RegisterResult::Created(stock) => stock.id,
            _ => panic!("표본에 없음"),
        };
    let samsung = register(Market::Kr, "005930", "삼전", None);
    let nvidia = register(Market::Us, "NVDA", "NVDA", Some("엔비디아"));
    replace_keywords(
        &mut conn,
        samsung,
        &[
            node(1, None, 1, "HBM 메모리", None, "삼성전자 → HBM 메모리"),
            node(
                2,
                Some(1),
                2,
                "엔비디아",
                None,
                "삼성전자 → HBM 메모리 → 엔비디아",
            ),
        ],
        now(),
    )
    .unwrap();
    replace_keywords(
        &mut conn,
        nvidia,
        &[node(
            1,
            None,
            1,
            "AI 가속기",
            Some("AI accelerator"),
            "엔비디아 → AI 가속기",
        )],
        now(),
    )
    .unwrap();
    Ids { samsung, nvidia }
}

fn add_stocks(shared: &SharedTestDb) -> Ids {
    add_stocks_at(shared, now())
}

/// `Date.prototype.toUTCString()` 모양: "Tue, 20 Oct 2026 03:00:00 GMT"
fn utc_string(at: EpochMs) -> String {
    chrono::DateTime::from_timestamp_millis(at)
        .unwrap()
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string()
}

fn item(title: &str, link: &str, pub_date: Option<EpochMs>, description: &str) -> String {
    let date = pub_date.map_or_else(String::new, |d| {
        format!("<pubDate>{}</pubDate>", utc_string(d))
    });
    format!(
        "<item><title>{title}</title><link>{link}</link><description>{description}</description>{date}</item>"
    )
}

fn simple_item(title: &str, link: &str, pub_date: Option<EpochMs>) -> String {
    item(title, link, pub_date, "")
}

fn rss(items: &[String]) -> String {
    format!(
        r#"<rss version="2.0"><channel>{}</channel></rss>"#,
        items.concat()
    )
}

#[derive(Clone)]
enum Route {
    Reply(u16, String),
    Fail,
}

fn ok(text: String) -> Route {
    Route::Reply(200, text)
}

struct FakeHttp {
    routes: HashMap<String, Route>,
    calls: Mutex<Vec<String>>,
}

impl HttpClient for FakeHttp {
    async fn get(&self, url: &str, _: &[(&str, &str)]) -> Result<HttpResponse, HttpError> {
        self.calls.lock().unwrap().push(url.to_string());
        match self.routes.get(url) {
            None => Ok(HttpResponse {
                status: 404,
                text: String::new(),
                content_type: String::new(),
            }),
            Some(Route::Fail) => Err(HttpError::new("연결 실패", HttpErrorKind::Network)),
            Some(Route::Reply(status, text)) => Ok(HttpResponse {
                status: *status,
                text: text.clone(),
                content_type: "application/rss+xml".into(),
            }),
        }
    }
}

/// 모두 허용하거나, 주소가 `blocked_prefix`로 시작하면 막고 `unknown_prefix`로 시작하면 확인하지 못하는 가짜
struct FakeRobots {
    blocked_prefix: Option<&'static str>,
    unknown_prefix: Option<&'static str>,
}

impl RobotsCheck for FakeRobots {
    async fn check(&self, url: &str) -> RobotsVerdict {
        if self
            .blocked_prefix
            .is_some_and(|prefix| url.starts_with(prefix))
        {
            RobotsVerdict::Disallowed
        } else if self
            .unknown_prefix
            .is_some_and(|prefix| url.starts_with(prefix))
        {
            RobotsVerdict::Unknown("연결하지 못했습니다: handshake failure".into())
        } else {
            RobotsVerdict::Allowed
        }
    }
}

type CompanyAnswer = Box<dyn Fn(&str) -> Vec<FinnhubNews> + Send + Sync>;

struct FakeNews {
    company: CompanyAnswer,
    general: Vec<FinnhubNews>,
    asked: Mutex<Vec<(String, EpochMs, EpochMs)>>,
}

impl FinnhubNewsClient for FakeNews {
    async fn company_news(
        &self,
        symbol: &str,
        from: EpochMs,
        to: EpochMs,
    ) -> Result<Vec<FinnhubNews>, FinnhubError> {
        self.asked
            .lock()
            .unwrap()
            .push((symbol.to_string(), from, to));
        Ok((self.company)(symbol))
    }

    async fn general_news(&self) -> Result<Vec<FinnhubNews>, FinnhubError> {
        Ok(self.general.clone())
    }
}

fn fake_news(
    company: impl Fn(&str) -> Vec<FinnhubNews> + Send + Sync + 'static,
    general: Vec<FinnhubNews>,
) -> FakeNews {
    FakeNews {
        company: Box::new(company),
        general,
        asked: Mutex::new(Vec::new()),
    }
}

fn news(headline: &str, id: i64, hours_ago: f64, summary: &str) -> FinnhubNews {
    FinnhubNews {
        id: Some(id as f64),
        headline: headline.into(),
        summary: Some(summary.into()),
        url: format!("https://finnhub.io/api/news?id={id}"),
        source: Some("Yahoo".into()),
        datetime: (ago(hours_ago) / 1000) as f64,
    }
}

fn feeds() -> Vec<RssFeed> {
    vec![
        RssFeed {
            id: "rss:a:economy".into(),
            publisher: "가경제".into(),
            section: "경제".into(),
            url: "https://a.example/economy.xml".into(),
        },
        RssFeed {
            id: "rss:b:all".into(),
            publisher: "나일보".into(),
            section: "전체".into(),
            url: "https://b.example/all.xml".into(),
        },
    ]
}

type TestDeps = CollectDeps<FakeHttp, FakeRobots, FakeNews, FakeDecider>;

struct Setup {
    feeds: Vec<RssFeed>,
    robots_blocked: Option<&'static str>,
    robots_unknown: Option<&'static str>,
    finnhub: Option<FakeNews>,
    decider: Option<FakeDecider>,
    now: EpochMs,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            feeds: feeds(),
            robots_blocked: None,
            robots_unknown: None,
            finnhub: None,
            decider: None,
            now: now(),
        }
    }
}

fn build(shared: &SharedTestDb, routes: Vec<(&str, Route)>, setup: Setup) -> Arc<TestDeps> {
    let at = setup.now;
    Arc::new(CollectDeps {
        db: shared.db.clone(),
        http: Arc::new(FakeHttp {
            routes: routes
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            calls: Mutex::new(Vec::new()),
        }),
        robots: FakeRobots {
            blocked_prefix: setup.robots_blocked,
            unknown_prefix: setup.robots_unknown,
        },
        feeds: setup.feeds,
        finnhub: setup.finnhub,
        decider: setup.decider,
        now: Some(Arc::new(move || at)),
    })
}

async fn run(deps: &Arc<TestDeps>) -> RunSummary {
    start_collection_run(deps, RunTrigger::Manual)
        .unwrap()
        .1
        .await
        .unwrap()
}

#[allow(clippy::type_complexity)]
fn query<T>(
    shared: &SharedTestDb,
    sql: &str,
    map: impl Fn(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
) -> Vec<T> {
    let conn = shared.db.lock().unwrap();
    let mut stmt = conn.prepare(sql).unwrap();
    stmt.query_map([], map)
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn count(shared: &SharedTestDb, table: &str) -> i64 {
    shared
        .db
        .lock()
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

struct RunRecord {
    trigger: String,
    status: String,
    since: i64,
    finished_at: Option<i64>,
    articles_new: i64,
    matches_new: i64,
    note: Option<String>,
    sources: Vec<SourceResult>,
    evaluation: Option<Value>,
}

fn run_records(shared: &SharedTestDb) -> Vec<RunRecord> {
    query(
        shared,
        "SELECT trigger, status, since, finished_at, articles_new, matches_new, note, sources, evaluation FROM collection_runs ORDER BY id",
        |r| {
            let sources: String = r.get(7)?;
            let evaluation: Option<String> = r.get(8)?;
            Ok(RunRecord {
                trigger: r.get(0)?,
                status: r.get(1)?,
                since: r.get(2)?,
                finished_at: r.get(3)?,
                articles_new: r.get(4)?,
                matches_new: r.get(5)?,
                note: r.get(6)?,
                sources: serde_json::from_str::<Vec<Value>>(&sources)
                    .unwrap()
                    .iter()
                    .filter_map(SourceResult::from_json)
                    .collect(),
                evaluation: evaluation.map(|e| serde_json::from_str(&e).unwrap()),
            })
        },
    )
}

fn article_titles(shared: &SharedTestDb) -> Vec<(String, String, String)> {
    query(
        shared,
        "SELECT title, publisher, feed FROM articles ORDER BY id",
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
}

// describe("수집 회차 (SPEC FR-4)")
#[tokio::test(flavor = "current_thread")]
async fn first_run_saves_only_items_matching_stocks_within_24_hours_and_merges_same_url() {
    // it("첫 회차: 최근 24시간 기사 중 종목에 걸린 것만 저장하고, 같은 주소는 한 기사로 합친다")
    let shared = seeded();
    let ids = add_stocks(&shared);
    let deps = build(
        &shared,
        vec![
            (
                "https://a.example/economy.xml",
                ok(rss(&[
                    simple_item(
                        "삼성전자 3분기 실적",
                        "https://news.example/1?utm_source=rss",
                        Some(ago(2.0)),
                    ),
                    simple_item(
                        "메모리 가격 오르자 HBM 증설",
                        "https://news.example/2",
                        Some(ago(3.0)),
                    ),
                    simple_item("날씨 소식", "https://news.example/3", Some(ago(1.0))),
                    simple_item(
                        "삼성전자 오래된 기사",
                        "https://news.example/old",
                        Some(ago(30.0)),
                    ),
                ])),
            ),
            // 같은 기사(추적 파라미터만 다름)와 미국 종목 기사
            (
                "https://b.example/all.xml",
                ok(rss(&[
                    simple_item(
                        "삼성전자 3분기 실적",
                        "http://news.example/1/",
                        Some(ago(2.0)),
                    ),
                    simple_item(
                        "엔비디아, 새 AI 가속기 공개",
                        "https://news.example/4",
                        Some(ago(1.0)),
                    ),
                ])),
            ),
        ],
        Setup::default(),
    );

    let summary = run(&deps).await;

    assert_eq!(
        (summary.status, summary.articles_new, summary.matches_new),
        (RunStatus::Success, 3, 4)
    );
    assert_eq!(
        article_titles(&shared),
        [
            (
                "삼성전자 3분기 실적".to_string(),
                "가경제".to_string(),
                "rss:a:economy".to_string()
            ),
            (
                "메모리 가격 오르자 HBM 증설".to_string(),
                "가경제".to_string(),
                "rss:a:economy".to_string()
            ),
            (
                "엔비디아, 새 AI 가속기 공개".to_string(),
                "나일보".to_string(),
                "rss:b:all".to_string()
            ),
        ]
    );
    let article_ids: Vec<i64> = query(&shared, "SELECT id FROM articles ORDER BY id", |r| r.get(0));
    let links: Vec<(i64, i64, String, String, i64)> = query(
        &shared,
        "SELECT article_id, stock_id, keyword_path, match_kind, level FROM article_matches ORDER BY id",
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    );
    assert_eq!(
        links,
        [
            (
                article_ids[0],
                ids.samsung,
                "삼성전자".to_string(),
                "name".to_string(),
                0
            ),
            (
                article_ids[1],
                ids.samsung,
                "삼성전자 → HBM 메모리".to_string(),
                "keyword".to_string(),
                1
            ),
            // '엔비디아'는 삼성전자의 2단계 키워드이자 엔비디아 종목 이름
            (
                article_ids[2],
                ids.samsung,
                "삼성전자 → HBM 메모리 → 엔비디아".to_string(),
                "keyword".to_string(),
                2
            ),
            (
                article_ids[2],
                ids.nvidia,
                "엔비디아".to_string(),
                "name".to_string(),
                0
            ),
        ]
    );

    let record = &run_records(&shared)[0];
    assert_eq!(
        (record.trigger.as_str(), record.status.as_str()),
        ("manual", "success")
    );
    assert_eq!((record.since, record.finished_at), (ago(24.0), Some(now())));
    assert_eq!(
        (record.articles_new, record.matches_new, record.note.clone()),
        (3, 4, None)
    );
    // 걸린 시간(ms)은 실행마다 달라 비교에서 뺀다.
    let sources: Vec<SourceResult> = record
        .sources
        .iter()
        .map(|s| SourceResult { ms: 0, ..s.clone() })
        .collect();
    let expected =
        |id: &str, label: &str, group: &str, fetched, fresh, saved, matched| SourceResult {
            id: id.into(),
            label: label.into(),
            group: group.into(),
            fetched,
            fresh,
            saved,
            matched,
            since: None,
            error: None,
            ms: 0,
        };
    assert_eq!(
        sources,
        [
            expected("rss:a:economy", "가경제 경제", "가경제", 4, 3, 2, 2),
            expected("rss:b:all", "나일보 전체", "나일보", 2, 2, 1, 2)
        ]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn one_failing_source_makes_partial_and_all_failing_makes_failed() {
    // it("한 소스가 실패해도 나머지는 계속하고 '일부 실패'로 남긴다. 모두 실패하면 '실패'")
    let shared = seeded();
    add_stocks(&shared);
    let partial = build(
        &shared,
        vec![
            (
                "https://a.example/economy.xml",
                Route::Reply(500, String::new()),
            ),
            (
                "https://b.example/all.xml",
                ok(rss(&[simple_item(
                    "삼성전자 소식",
                    "https://n.example/1",
                    Some(ago(1.0)),
                )])),
            ),
        ],
        Setup::default(),
    );
    let summary = run(&partial).await;
    assert_eq!(
        (summary.status, summary.articles_new),
        (RunStatus::Partial, 1)
    );
    let record = &run_records(&shared)[0];
    assert_eq!(record.sources[0].id, "rss:a:economy");
    assert_eq!(record.sources[0].error.as_deref(), Some("HTTP 500"));
    assert_eq!(record.sources[0].saved, 0);

    let all_fail = build(
        &shared,
        vec![
            ("https://a.example/economy.xml", Route::Fail),
            (
                "https://b.example/all.xml",
                Route::Reply(503, String::new()),
            ),
        ],
        Setup::default(),
    );
    assert_eq!(run(&all_fail).await.status, RunStatus::Failed);
}

#[tokio::test(flavor = "current_thread")]
async fn robots_block_means_no_download_and_the_reason_is_recorded() {
    // it("robots.txt가 막으면 받지 않고 그 이유를 남긴다")
    let shared = seeded();
    add_stocks(&shared);
    let blocked = build(
        &shared,
        vec![("https://b.example/all.xml", ok(rss(&[])))],
        Setup {
            robots_blocked: Some("https://a.example"),
            ..Setup::default()
        },
    );
    run(&blocked).await;
    assert_eq!(
        *blocked.http.calls.lock().unwrap(),
        ["https://b.example/all.xml"]
    );
    assert!(
        run_records(&shared)[0].sources[0]
            .error
            .as_deref()
            .unwrap()
            .contains("robots.txt")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn robots_unknown_is_told_apart_from_blocked_and_keeps_the_cause() {
    // robots.txt를 확인하지 못한 것(연결 실패 등)은 막힌 것과 다른 문구로, 실제 원인을 함께 남긴다.
    let shared = seeded();
    add_stocks(&shared);
    let deps = build(
        &shared,
        vec![("https://b.example/all.xml", ok(rss(&[])))],
        Setup {
            robots_unknown: Some("https://a.example"),
            ..Setup::default()
        },
    );
    run(&deps).await;
    assert_eq!(
        *deps.http.calls.lock().unwrap(),
        ["https://b.example/all.xml"]
    );
    assert_eq!(
        run_records(&shared)[0].sources[0].error.as_deref(),
        Some("robots.txt를 확인하지 못해 건너뛰었습니다: 연결하지 못했습니다: handshake failure")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn without_stocks_nothing_is_downloaded() {
    // it("등록한 종목이 없으면 아무것도 받지 않는다")
    let shared = seeded();
    let deps = build(&shared, vec![], Setup::default());
    let summary = run(&deps).await;
    assert_eq!(
        (summary.status, summary.articles_new),
        (RunStatus::Success, 0)
    );
    assert!(deps.http.calls.lock().unwrap().is_empty());
    assert!(
        run_records(&shared)[0]
            .note
            .as_deref()
            .unwrap()
            .contains("종목이 없어")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn deleted_stock_gets_no_links() {
    // it("삭제한 종목에는 걸지 않는다")
    let shared = seeded();
    let ids = add_stocks(&shared);
    delete_stock(&shared.db.lock().unwrap(), ids.samsung, now()).unwrap();
    let deps = build(
        &shared,
        vec![(
            "https://a.example/economy.xml",
            ok(rss(&[simple_item(
                "삼성전자 소식",
                "https://n.example/1",
                Some(ago(1.0)),
            )])),
        )],
        Setup::default(),
    );
    assert_eq!(run(&deps).await.articles_new, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn item_without_publish_time_is_saved_with_first_seen_time_and_only_once() {
    // it("발행 시각이 없는 기사는 처음 받은 시각으로 저장하고, 다시 받아도 한 번만 저장한다")
    let shared = seeded();
    add_stocks(&shared);
    let routes = || {
        vec![(
            "https://a.example/economy.xml",
            ok(rss(&[simple_item(
                "삼성전자 단독",
                "https://n.example/x",
                None,
            )])),
        )]
    };
    run(&build(&shared, routes(), Setup::default())).await;
    run(&build(
        &shared,
        routes(),
        Setup {
            now: now() + HOUR,
            ..Setup::default()
        },
    ))
    .await;
    assert_eq!(count(&shared, "articles"), 1);
    let published: i64 = shared
        .db
        .lock()
        .unwrap()
        .query_row("SELECT published_at FROM articles", [], |r| r.get(0))
        .unwrap();
    assert_eq!(published, now());
    assert_eq!(count(&shared, "article_matches"), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn publish_time_more_than_ten_minutes_ahead_is_saved_as_seen_time() {
    // it("발행 시각이 받은 시각보다 10분 넘게 미래면(국민일보 지면 기사) 받은 시각으로 저장한다")
    let shared = seeded();
    add_stocks(&shared);
    let deps = build(
        &shared,
        vec![(
            "https://a.example/economy.xml",
            ok(rss(&[
                simple_item(
                    "삼성전자 지면 기사",
                    "https://n.example/f1",
                    Some(now() + 19 * HOUR),
                ),
                simple_item(
                    "삼성전자 조금 빠른 시계",
                    "https://n.example/f2",
                    Some(now() + 5 * 60_000),
                ),
            ])),
        )],
        Setup::default(),
    );
    run(&deps).await;
    let saved: HashMap<String, i64> =
        query(&shared, "SELECT title, published_at FROM articles", |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .into_iter()
        .collect();
    assert_eq!(saved["삼성전자 지면 기사"], now());
    assert_eq!(saved["삼성전자 조금 빠른 시계"], now() + 5 * 60_000);
}

#[tokio::test(flavor = "current_thread")]
async fn whole_body_in_summary_is_only_matched_on_first_300_chars() {
    // it("요약 자리에 본문이 통째로 와도 앞 300자까지만 보고 건다")
    let shared = seeded();
    add_stocks(&shared);
    let deps = build(
        &shared,
        vec![(
            "https://a.example/economy.xml",
            ok(rss(&[item(
                "철강 생산 소식",
                "https://n.example/steel",
                Some(ago(1.0)),
                &format!("{} 삼성전자", "가".repeat(400)),
            )])),
        )],
        Setup::default(),
    );
    assert_eq!(run(&deps).await.articles_new, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn summary_is_stored_up_to_300_chars() {
    // it("요약은 300자까지만 저장한다(본문은 저장하지 않음)")
    let shared = seeded();
    add_stocks(&shared);
    let deps = build(
        &shared,
        vec![(
            "https://a.example/economy.xml",
            ok(rss(&[item(
                "삼성전자 긴 기사",
                "https://n.example/long",
                Some(ago(1.0)),
                &"가".repeat(1000),
            )])),
        )],
        Setup::default(),
    );
    run(&deps).await;
    let summary: String = shared
        .db
        .lock()
        .unwrap()
        .query_row("SELECT summary FROM articles", [], |r| r.get(0))
        .unwrap();
    assert_eq!(summary.chars().count(), SUMMARY_MAX_CHARS);
    assert!(summary.ends_with('…'));
}

// describe("수집 직후 평가 (5단계)")
fn evaluation_routes() -> Vec<(&'static str, Route)> {
    vec![(
        "https://a.example/economy.xml",
        ok(rss(&[
            simple_item("삼성전자 3분기 실적", "https://n.example/1", Some(ago(1.0))),
            simple_item(
                "엔비디아, 새 AI 가속기 공개",
                "https://n.example/2",
                Some(ago(2.0)),
            ),
        ])),
    )]
}

#[tokio::test(flavor = "current_thread")]
async fn new_links_are_evaluated_by_jev_and_the_result_is_recorded() {
    // it("새 연결을 Jev로 평가하고 회차 기록에 평가 결과를 남긴다")
    let shared = seeded();
    add_stocks(&shared);
    let deps = build(
        &shared,
        evaluation_routes(),
        Setup {
            feeds: vec![feeds()[0].clone()],
            decider: Some(fake_article_decider(|_| Ok(fake_decision()))),
            ..Setup::default()
        },
    );

    assert_eq!(run(&deps).await.matches_new, 3);

    assert_eq!(
        deps.decider.as_ref().unwrap().calls.lock().unwrap().len(),
        3
    );
    assert_eq!(count(&shared, "evaluations"), 3);
    let evaluation = run_records(&shared)[0].evaluation.clone().unwrap();
    assert_eq!(evaluation["evaluated"], 3);
    assert_eq!(evaluation["relevant"], 3);
    assert_eq!(evaluation["failed"], 0);
    assert_eq!(evaluation["remaining"], 0);
}

#[tokio::test(flavor = "current_thread")]
async fn without_key_only_collection_runs_and_the_reason_is_recorded() {
    // it("키가 없으면 수집만 하고 평가하지 않은 이유를 남긴다")
    let shared = seeded();
    add_stocks(&shared);
    let deps = build(
        &shared,
        evaluation_routes(),
        Setup {
            feeds: vec![feeds()[0].clone()],
            ..Setup::default()
        },
    );

    let summary = run(&deps).await;
    assert_eq!(
        (summary.status, summary.matches_new),
        (RunStatus::Success, 3)
    );
    let evaluation = run_records(&shared)[0].evaluation.clone().unwrap();
    assert_eq!(evaluation["evaluated"], 0);
    assert_eq!(evaluation["remaining"], 3);
    assert_eq!(
        evaluation["note"],
        "TYPESAFE_API_KEY가 없어 평가하지 않았습니다."
    );
}

// describe("기준 시각 (D-20)")
fn insert_run(
    shared: &SharedTestDb,
    trigger: &str,
    status: &str,
    started_at: EpochMs,
    since: EpochMs,
    sources: Option<&str>,
) {
    shared
        .db
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO collection_runs (trigger, status, started_at, since, sources) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![trigger, status, started_at, since, sources.unwrap_or("[]")],
        )
        .unwrap();
}

#[test]
fn since_is_24_hours_first_then_last_run_start_minus_one_hour_max_7_days() {
    // it("처음엔 24시간 전, 이후엔 지난 회차 시작 1시간 전, 최대 7일")
    let shared = seeded();
    let since = |at: EpochMs| compute_since(&shared.db.lock().unwrap(), at).unwrap();
    assert_eq!(since(now()), ago(24.0));

    insert_run(&shared, "manual", "success", ago(3.0), ago(27.0), None);
    // 실패한 회차와 도는 중인 회차는 기준이 되지 않는다.
    insert_run(&shared, "manual", "failed", ago(1.0), ago(4.0), None);
    assert_eq!(since(now()), ago(4.0));

    let later = now() + 30 * 24 * HOUR;
    assert_eq!(since(later), later - 7 * 24 * HOUR);
}

// describe("소스별 기준 시각 (D-24)")
fn source_json(id: &str, error: bool) -> Value {
    let mut result = SourceResult {
        id: id.into(),
        label: id.into(),
        group: id.into(),
        fetched: 1,
        fresh: 1,
        saved: 0,
        matched: 0,
        since: None,
        error: None,
        ms: 1,
    };
    if error {
        result.error = Some("HTTP 500".into());
    }
    result.to_json()
}

fn sources_text(list: &[Value]) -> String {
    Value::Array(list.to_vec()).to_string()
}

#[test]
fn failed_sources_resume_from_last_success_new_sources_from_run_since() {
    // it("지난번에 실패한 소스는 마지막으로 성공한 회차부터, 처음 보는 소스는 회차 기준 시각부터")
    let shared = seeded();
    let day = 24 * HOUR;
    let ok = |id: &str| source_json(id, false);
    let failed = |id: &str| source_json(id, true);
    let insert = |started_at: EpochMs, status: &str, list: Vec<Value>| {
        insert_run(
            &shared,
            "scheduled",
            status,
            started_at,
            started_at,
            Some(&sources_text(&list)),
        );
    };
    // 8일 전에 마지막으로 성공한 e는 7일 안에 성공 기록이 없다.
    insert(now() - 8 * day, "success", vec![ok("e")]);
    insert(ago(5.0), "success", vec![ok("a"), ok("b")]);
    insert(ago(3.0), "partial", vec![ok("a"), failed("b")]);
    insert(ago(2.0), "failed", vec![failed("a"), failed("b")]);
    insert(
        ago(1.0),
        "partial",
        vec![ok("a"), failed("b"), failed("c"), failed("e")],
    );

    let conn = shared.db.lock().unwrap();
    let run_since = compute_since(&conn, now()).unwrap();
    assert_eq!(run_since, ago(2.0));
    let since_for = source_since_resolver(&conn, now(), run_since).unwrap();
    assert_eq!(since_for.since_for("a"), ago(2.0));
    // b는 5시간 전 회차가 마지막 성공
    assert_eq!(since_for.since_for("b"), ago(6.0));
    // 7일 동안 한 번도 성공하지 못한 소스는 최대 기간(7일)
    assert_eq!(since_for.since_for("c"), now() - 7 * day);
    assert_eq!(since_for.since_for("e"), now() - 7 * day);
    // 처음 보는 소스(새 피드, 새로 등록한 미국 종목)
    assert_eq!(since_for.since_for("new"), ago(2.0));
}

#[tokio::test(flavor = "current_thread")]
async fn recovered_source_gets_items_left_in_feed_meanwhile_and_records_its_since() {
    // it("실패했던 소스가 돌아오면 그 사이 피드에 남은 기사를 받고, 쓴 기준 시각을 남긴다")
    let shared = seeded();
    add_stocks(&shared);
    // 4시간 전 회차: 두 소스 모두 성공
    run(&build(
        &shared,
        vec![
            ("https://a.example/economy.xml", ok(rss(&[]))),
            ("https://b.example/all.xml", ok(rss(&[]))),
        ],
        Setup {
            now: ago(4.0),
            ..Setup::default()
        },
    ))
    .await;
    // 2시간 전 회차: b만 실패
    run(&build(
        &shared,
        vec![
            ("https://a.example/economy.xml", ok(rss(&[]))),
            (
                "https://b.example/all.xml",
                Route::Reply(503, String::new()),
            ),
        ],
        Setup {
            now: ago(2.0),
            ..Setup::default()
        },
    ))
    .await;
    // 지금 회차: 회차 기준 시각은 3시간 전(2시간 전 회차 − 1시간).
    // b의 3.5시간 전 기사는 b의 기준 시각(4시간 전 회차 − 1시간 = 5시간 전) 안이라 받는다.
    let deps = build(
        &shared,
        vec![
            (
                "https://a.example/economy.xml",
                ok(rss(&[simple_item(
                    "삼성전자 a 소식",
                    "https://n.example/a35",
                    Some(ago(3.5)),
                )])),
            ),
            (
                "https://b.example/all.xml",
                ok(rss(&[simple_item(
                    "삼성전자 b 소식",
                    "https://n.example/b35",
                    Some(ago(3.5)),
                )])),
            ),
        ],
        Setup::default(),
    );

    run(&deps).await;

    let titles: Vec<String> = query(&shared, "SELECT title FROM articles", |r| r.get(0));
    assert_eq!(titles, ["삼성전자 b 소식"]);
    let last = run_records(&shared).pop().unwrap();
    assert_eq!(last.since, ago(3.0));
    assert_eq!(last.sources[0].since, None);
    assert_eq!(last.sources[1].since, Some(ago(5.0)));
}

// describe("새로 등록한 종목의 지난 기사 (D-20 보완, 2026-10-21 결정)")
fn add_hynix(shared: &SharedTestDb, at: EpochMs) -> i64 {
    let conn = shared.db.lock().unwrap();
    match register_stock(
        &conn,
        &RegisterInput {
            market: Market::Kr,
            code: "000660",
            user_input: "하닉",
            name_ko: None,
        },
        at,
    )
    .unwrap()
    {
        RegisterResult::Created(stock) => stock.id,
        _ => panic!("표본에 없음"),
    }
}

fn backfill_of(shared: &SharedTestDb, id: i64) -> Option<i64> {
    shared
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT backfill_from FROM stocks WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap()
}

fn feed_route(items: &[String]) -> Vec<(&'static str, Route)> {
    vec![("https://a.example/economy.xml", ok(rss(items)))]
}

fn link_titles(shared: &SharedTestDb) -> Vec<(String, i64)> {
    query(
        shared,
        "SELECT articles.title, article_matches.stock_id FROM article_matches \
         INNER JOIN articles ON articles.id = article_matches.article_id ORDER BY article_matches.id",
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
}

#[tokio::test(flavor = "current_thread")]
async fn next_run_links_only_the_new_stock_back_24_hours_and_clears_the_mark() {
    // it("다음 회차에서 그 종목만 등록 24시간 전 기사까지 걸고, 한 번 걸면 표시를 지운다")
    let shared = seeded();
    let ids = add_stocks_at(&shared, ago(10.0));
    // 삼성전자는 이미 수집해 온 종목
    run(&build(
        &shared,
        feed_route(&[]),
        Setup {
            feeds: vec![feeds()[0].clone()],
            now: ago(5.0),
            ..Setup::default()
        },
    ))
    .await;
    let hynix = add_hynix(&shared, ago(1.0));
    set_keyword_status(
        &shared.db.lock().unwrap(),
        hynix,
        crate::server::db::schema::KeywordStatus::Ready,
        None,
    )
    .unwrap();
    assert_eq!(backfill_of(&shared, hynix), Some(ago(25.0)));

    run(&build(
        &shared,
        feed_route(&[
            simple_item(
                "SK하이닉스 HBM 증설",
                "https://n.example/h20",
                Some(ago(20.0)),
            ),
            simple_item(
                "삼성전자와 SK하이닉스 협력",
                "https://n.example/b20",
                Some(ago(20.0)),
            ),
            simple_item(
                "삼성전자 지난 소식",
                "https://n.example/s20",
                Some(ago(20.0)),
            ),
            simple_item(
                "SK하이닉스 오래된 소식",
                "https://n.example/h30",
                Some(ago(30.0)),
            ),
            simple_item("삼성전자 새 소식", "https://n.example/s1", Some(ago(0.5))),
        ]),
        Setup {
            feeds: vec![feeds()[0].clone()],
            ..Setup::default()
        },
    ))
    .await;

    assert_eq!(
        link_titles(&shared),
        [
            ("SK하이닉스 HBM 증설".to_string(), hynix),
            // 기준 시각 전 기사는 새 종목에만 건다.
            ("삼성전자와 SK하이닉스 협력".to_string(), hynix),
            ("삼성전자 새 소식".to_string(), ids.samsung),
        ]
    );
    assert_eq!(backfill_of(&shared, hynix), None);

    // 다음 회차부터는 다른 종목과 같은 기준 시각
    run(&build(
        &shared,
        feed_route(&[simple_item(
            "SK하이닉스 다른 지난 소식",
            "https://n.example/h22",
            Some(ago(22.0)),
        )]),
        Setup {
            feeds: vec![feeds()[0].clone()],
            now: now() + HOUR,
            ..Setup::default()
        },
    ))
    .await;
    assert_eq!(link_titles(&shared).len(), 3);
}

#[tokio::test(flavor = "current_thread")]
async fn generating_keywords_postpone_backfill_and_an_all_failed_run_retries_next_time() {
    // it("키워드를 만드는 중이면 다음 회차로 미루고, 회차가 모두 실패하면 다음 회차에 다시 한다")
    let shared = seeded();
    add_stocks_at(&shared, ago(10.0));
    run(&build(
        &shared,
        feed_route(&[]),
        Setup {
            feeds: vec![feeds()[0].clone()],
            now: ago(5.0),
            ..Setup::default()
        },
    ))
    .await;
    let hynix = add_hynix(&shared, ago(1.0));
    set_keyword_status(
        &shared.db.lock().unwrap(),
        hynix,
        crate::server::db::schema::KeywordStatus::Generating,
        None,
    )
    .unwrap();
    let old = || {
        simple_item(
            "SK하이닉스 HBM 증설",
            "https://n.example/h20",
            Some(ago(20.0)),
        )
    };

    run(&build(
        &shared,
        feed_route(&[old()]),
        Setup {
            feeds: vec![feeds()[0].clone()],
            ..Setup::default()
        },
    ))
    .await;
    assert!(link_titles(&shared).is_empty());
    assert_eq!(backfill_of(&shared, hynix), Some(ago(25.0)));

    set_keyword_status(
        &shared.db.lock().unwrap(),
        hynix,
        crate::server::db::schema::KeywordStatus::Ready,
        None,
    )
    .unwrap();
    run(&build(
        &shared,
        vec![(
            "https://a.example/economy.xml",
            Route::Reply(503, String::new()),
        )],
        Setup {
            feeds: vec![feeds()[0].clone()],
            ..Setup::default()
        },
    ))
    .await;
    assert_eq!(backfill_of(&shared, hynix), Some(ago(25.0)));

    run(&build(
        &shared,
        feed_route(&[old()]),
        Setup {
            feeds: vec![feeds()[0].clone()],
            ..Setup::default()
        },
    ))
    .await;
    assert_eq!(
        link_titles(&shared),
        [("SK하이닉스 HBM 증설".to_string(), hynix)]
    );
    assert_eq!(backfill_of(&shared, hynix), None);
}

// describe("Finnhub (미국 뉴스)")
#[tokio::test(flavor = "current_thread")]
async fn company_news_links_directly_and_general_news_uses_english_keywords() {
    // it("종목 뉴스는 그 종목에 바로 걸고, 시장 뉴스는 영어 키워드로 건다")
    let shared = seeded();
    let ids = add_stocks(&shared);
    let finnhub = fake_news(
        |_| vec![news("Chip stocks rally", 1, 2.0, "")],
        vec![
            news("Demand for AI accelerator chips surges", 2, 1.0, ""),
            news("Oil prices fall", 3, 1.0, ""),
        ],
    );
    let deps = build(
        &shared,
        vec![],
        Setup {
            feeds: vec![],
            finnhub: Some(finnhub),
            ..Setup::default()
        },
    );

    let summary = run(&deps).await;
    assert_eq!(
        (summary.status, summary.articles_new, summary.matches_new),
        (RunStatus::Success, 2, 2)
    );

    assert_eq!(
        *deps.finnhub.as_ref().unwrap().asked.lock().unwrap(),
        [("NVDA".to_string(), ago(24.0), now())]
    );
    #[allow(clippy::type_complexity)]
    let rows: Vec<(String, String, String, i64, String, String)> = query(
        &shared,
        "SELECT articles.title, articles.publisher, articles.feed, article_matches.stock_id, article_matches.keyword_path, \
         article_matches.match_kind FROM article_matches INNER JOIN articles ON articles.id = article_matches.article_id \
         ORDER BY article_matches.id",
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        },
    );
    assert_eq!(
        rows,
        [
            (
                "Chip stocks rally".to_string(),
                "Yahoo".to_string(),
                "finnhub:company:NVDA".to_string(),
                ids.nvidia,
                "엔비디아".to_string(),
                "direct".to_string()
            ),
            (
                "Demand for AI accelerator chips surges".to_string(),
                "Yahoo".to_string(),
                "finnhub:general".to_string(),
                ids.nvidia,
                "엔비디아 → AI 가속기".to_string(),
                "keyword".to_string()
            ),
        ]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn newly_registered_us_stock_asks_company_news_from_24_hours_before_registration() {
    // it("새로 등록한 미국 종목은 종목 뉴스를 등록 24시간 전부터 묻는다")
    let shared = seeded();
    add_stocks_at(&shared, ago(10.0));
    let quiet = || fake_news(|_| vec![], vec![]);
    run(&build(
        &shared,
        vec![],
        Setup {
            feeds: vec![],
            finnhub: Some(quiet()),
            now: ago(5.0),
            ..Setup::default()
        },
    ))
    .await;
    {
        let conn = shared.db.lock().unwrap();
        let RegisterResult::Created(stock) = register_stock(
            &conn,
            &RegisterInput {
                market: Market::Us,
                code: "TSLA",
                user_input: "TSLA",
                name_ko: Some("테슬라"),
            },
            ago(1.0),
        )
        .unwrap() else {
            panic!("표본에 없음");
        };
        set_keyword_status(
            &conn,
            stock.id,
            crate::server::db::schema::KeywordStatus::Ready,
            None,
        )
        .unwrap();
    }

    let deps = build(
        &shared,
        vec![],
        Setup {
            feeds: vec![],
            finnhub: Some(quiet()),
            ..Setup::default()
        },
    );
    run(&deps).await;

    assert_eq!(
        *deps.finnhub.as_ref().unwrap().asked.lock().unwrap(),
        [
            // 이미 수집해 온 엔비디아는 회차 기준 시각(지난 회차 1시간 전)부터
            ("NVDA".to_string(), ago(6.0), now()),
            ("TSLA".to_string(), ago(25.0), now()),
        ]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn titles_of_unmatched_items_are_kept_for_rss_and_general_news_only() {
    // it("종목에 안 걸린 기사도 제목을 남긴다: 언론사 RSS·시장 뉴스만, 미국 종목별 뉴스는 빼고(10단계)")
    let shared = seeded();
    add_stocks(&shared);
    // 지난 회차에 남은 4일 전 제목은 이번 회차에 지운다.
    shared
        .db
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO feed_headlines (title_key, title, publisher, feed, published_at, seen_at) \
             VALUES ('지난제목', '지난 제목', 'p', 'rss:a:economy', ?1, ?1)",
            [ago(96.0)],
        )
        .unwrap();
    let finnhub = fake_news(
        |_| vec![news("Nvidia unveils chip", 11, 2.0, "")],
        vec![news("Oil prices fall", 12, 1.0, "")],
    );
    let deps = build(
        &shared,
        vec![(
            "https://a.example/economy.xml",
            ok(rss(&[
                simple_item(
                    "삼성전자 3분기 실적",
                    "https://news.example/1",
                    Some(ago(2.0)),
                ),
                simple_item(
                    "한은 기준금리 동결",
                    "https://news.example/2",
                    Some(ago(1.0)),
                ),
            ])),
        )],
        Setup {
            finnhub: Some(finnhub),
            ..Setup::default()
        },
    );
    run(&deps).await;

    let kept: Vec<(String, String)> = query(
        &shared,
        "SELECT title, feed FROM feed_headlines ORDER BY title",
        |r| Ok((r.get(0)?, r.get(1)?)),
    );
    assert_eq!(
        kept,
        [
            ("Oil prices fall".to_string(), "finnhub:general".to_string()),
            (
                "삼성전자 3분기 실적".to_string(),
                "rss:a:economy".to_string()
            ),
            (
                "한은 기준금리 동결".to_string(),
                "rss:a:economy".to_string()
            ),
        ]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn without_finnhub_us_news_is_skipped() {
    // it("키가 없으면(finnhub=null) 미국 뉴스는 건너뛴다")
    let shared = seeded();
    add_stocks(&shared);
    let deps = build(
        &shared,
        vec![],
        Setup {
            feeds: vec![],
            ..Setup::default()
        },
    );
    run(&deps).await;
    assert!(run_records(&shared)[0].sources.is_empty());
}

// describe("회차 관리")
#[test]
fn run_stopped_midway_is_closed_as_failed_at_startup() {
    // it("도중에 멈춘 회차는 시작할 때 실패로 정리한다")
    let shared = seeded();
    insert_run(&shared, "manual", "running", ago(1.0), ago(25.0), None);
    assert_eq!(
        close_stale_runs(&shared.db.lock().unwrap(), now()).unwrap(),
        1
    );
    let record = &run_records(&shared)[0];
    assert_eq!(
        (record.status.as_str(), record.finished_at),
        ("failed", Some(now()))
    );
    assert_eq!(
        record.note.as_deref(),
        Some("worker가 수집 도중 멈췄습니다.")
    );
}

/// 신호가 올 때까지 응답하지 않는 가짜 HTTP
struct GatedHttp {
    open: AtomicBool,
}

impl HttpClient for GatedHttp {
    async fn get(&self, _: &str, _: &[(&str, &str)]) -> Result<HttpResponse, HttpError> {
        while !self.open.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        Ok(HttpResponse {
            status: 200,
            text: rss(&[]),
            content_type: String::new(),
        })
    }
}

#[tokio::test(flavor = "current_thread")]
async fn runner_returns_none_while_running_and_can_start_again_after() {
    // it("실행기: 도는 동안 다시 시작하면 null, 끝나면 다시 시작할 수 있다")
    let shared = seeded();
    add_stocks(&shared);
    let deps: Arc<CollectDeps<GatedHttp, FakeRobots, FakeNews, FakeDecider>> =
        Arc::new(CollectDeps {
            db: shared.db.clone(),
            http: Arc::new(GatedHttp {
                open: AtomicBool::new(false),
            }),
            robots: FakeRobots {
                blocked_prefix: None,
                unknown_prefix: None,
            },
            feeds: feeds(),
            finnhub: None,
            decider: None,
            now: Some(Arc::new(now)),
        });
    let runner = CollectionRunner::new(deps.clone());
    let first = runner.start(RunTrigger::Manual);
    assert_eq!(first, Some(1));
    assert_eq!(runner.start(RunTrigger::Manual), None);
    assert_eq!(runner.running_id(), Some(1));
    deps.http.open.store(true, Ordering::SeqCst);
    runner.idle().await;
    assert_eq!(runner.running_id(), None);
    assert_eq!(runner.start(RunTrigger::Scheduled), Some(2));
    runner.idle().await;
    let triggers: Vec<String> = query(
        &shared,
        "SELECT trigger FROM collection_runs ORDER BY id",
        |r| r.get(0),
    );
    assert_eq!(triggers, ["manual", "scheduled"]);
}

#[test]
fn clip_cuts_to_max_with_ellipsis_in_utf16_units() {
    // Rust에서만 생기는 경우: `clip`(공백 정리 후 max를 넘으면 max-1자 + "…")
    assert_eq!(clip("  짧은 글  ", 10), "짧은 글");
    assert_eq!(clip(&"가".repeat(5), 5), "가가가가가");
    assert_eq!(clip(&"가".repeat(6), 5), "가가가가…");
    assert_eq!(clip("a😀bcd", 4), "a😀…");
}

#[test]
fn source_result_json_keeps_ts_key_order() {
    // Rust에서만 생기는 경우: 회차 기록의 소스 결과 JSON(키 순서 `ms` 뒤에 `error`)
    let mut result = SourceResult {
        id: "rss:a".into(),
        label: "가 경제".into(),
        group: "가".into(),
        fetched: 4,
        fresh: 3,
        saved: 2,
        matched: 2,
        since: Some(5),
        error: None,
        ms: 12,
    };
    assert_eq!(
        result.to_json().to_string(),
        r#"{"id":"rss:a","label":"가 경제","group":"가","fetched":4,"fresh":3,"saved":2,"matched":2,"since":5,"ms":12}"#
    );
    result.since = None;
    result.error = Some("HTTP 500".into());
    assert_eq!(
        result.to_json().to_string(),
        r#"{"id":"rss:a","label":"가 경제","group":"가","fetched":4,"fresh":3,"saved":2,"matched":2,"ms":12,"error":"HTTP 500"}"#
    );
}
