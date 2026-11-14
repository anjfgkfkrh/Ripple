use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

use crate::server::ai::decider::types::Decider;
use crate::server::collect::feed_headlines::{
    HeadlineItem, prune_feed_headlines, save_feed_headlines,
};
use crate::server::collect::feeds::RssFeed;
use crate::server::collect::finnhub::{FinnhubNews, FinnhubNewsClient};
use crate::server::collect::http::HttpClient;
use crate::server::collect::r#match::{
    MatchKeyword, MatchStock, Matcher, StockMatch, create_matcher,
};
use crate::server::collect::published::effective_published_at;
use crate::server::collect::robots::{RobotsCheck, RobotsVerdict};
use crate::server::collect::rss::parse_feed;
use crate::server::collect::url::url_key;
use crate::server::db::client::Db;
use crate::server::db::schema::{
    EvaluationSummary, ItemKind, MatchKind, RunStatus, RunTrigger, SourceResult,
};
use crate::server::evaluate::run::{EvaluateOptions, evaluate_pending};
use crate::server::log::{log, log_error};
use crate::server::text::{js_len, js_slice, js_to_fixed, js_trim};
use crate::server::time::{EpochMs, now_ms};

const HOUR_MS: i64 = 60 * 60 * 1000;
/// 처음 수집할 때 볼 기간(D-20)
pub const FIRST_RUN_WINDOW_MS: i64 = 24 * HOUR_MS;
/// 지난 회차와 겹쳐 볼 시간. 피드에 늦게 올라온 기사를 놓치지 않게 한다(중복은 주소로 거른다).
pub const OVERLAP_MS: i64 = HOUR_MS;
/// 오래 쉬었다가 다시 돌 때도 이보다 오래된 기사는 보지 않는다.
pub const MAX_WINDOW_MS: i64 = 7 * 24 * HOUR_MS;
/// 요약은 이만큼만 저장한다(본문은 저장하지 않음).
pub const SUMMARY_MAX_CHARS: usize = 300;
const TITLE_MAX_CHARS: usize = 300;

/// 소스가 돌려주는 기사 한 건
#[derive(Debug, Clone)]
pub struct CollectedItem {
    pub url: String,
    pub title: String,
    pub summary: String,
    pub publisher: String,
    pub published_at: Option<EpochMs>,
    /// 소스가 종목을 직접 지정한 경우(예: Finnhub 종목 뉴스)
    pub direct_stock_ids: Vec<i64>,
}

/// 걸린 기사 하나(저장할 것)
#[derive(Debug, Clone)]
pub struct KeepItem {
    pub item: CollectedItem,
    pub key: String,
    pub published_at: EpochMs,
    pub matches: Vec<StockMatch>,
}

enum SourceKind {
    Feed(RssFeed),
    FinnhubCompany { stock_id: i64, code: String },
    FinnhubGeneral,
}

struct Source {
    id: String,
    label: String,
    group: String,
    /// 받은 기사 제목을 토픽 갱신 재료로 남길지(언론사 RSS·Finnhub 시장 뉴스, 10단계)
    keep_headlines: bool,
    kind: SourceKind,
}

type NowFn = Arc<dyn Fn() -> EpochMs + Send + Sync>;

pub struct CollectDeps<H, R, F, D> {
    pub db: Db,
    pub http: Arc<H>,
    pub robots: R,
    pub feeds: Vec<RssFeed>,
    /// 키가 없으면 None: 미국 뉴스는 건너뛴다.
    pub finnhub: Option<F>,
    /// 키가 없으면 None: 수집만 하고 평가는 미룬다(5단계).
    pub decider: Option<D>,
    pub now: Option<NowFn>,
}

impl<H, R, F, D> CollectDeps<H, R, F, D> {
    fn now(&self) -> EpochMs {
        self.now.as_ref().map_or_else(now_ms, |f| f())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSummary {
    pub run_id: i64,
    pub status: RunStatus,
    pub articles_new: i64,
    pub matches_new: i64,
}

/// 수집 한 회차(SPEC FR-4). 소스마다 따로 처리해서 한 소스가 실패해도 나머지는 계속 돈다.
/// 종목에 하나라도 걸린 기사만 저장한다. 같은 원문 주소는 한 기사로 합친다(D-19).
/// 회차 기록을 만들고 바로 돌아오며, 나머지는 tokio 작업으로 돌린다(tokio 런타임 안에서만 부른다).
pub fn start_collection_run<H, R, F, D>(
    deps: &Arc<CollectDeps<H, R, F, D>>,
    trigger: RunTrigger,
) -> rusqlite::Result<(i64, tokio::task::JoinHandle<RunSummary>)>
where
    H: HttpClient + 'static,
    R: RobotsCheck + 'static,
    F: FinnhubNewsClient + 'static,
    D: Decider + 'static,
{
    let started_at = deps.now();
    let (since, run_id) = {
        let conn = deps.db.lock().expect("DB 잠금이 깨졌습니다");
        let since = compute_since(&conn, started_at)?;
        conn.execute(
            "INSERT INTO collection_runs (trigger, status, started_at, since) VALUES (?1, 'running', ?2, ?3)",
            params![trigger.as_str(), started_at, since],
        )?;
        (since, conn.last_insert_rowid())
    };
    let deps = deps.clone();
    let done = tokio::spawn(async move {
        match collect(&deps, run_id, started_at, since).await {
            Ok(summary) => summary,
            Err(error) => {
                log_error(&format!("수집 회차 {run_id} 실패"), &error);
                let conn = deps.db.lock().expect("DB 잠금이 깨졌습니다");
                if let Err(e) = finish_run(
                    &conn,
                    run_id,
                    deps.now(),
                    RunStatus::Failed,
                    &[],
                    0,
                    0,
                    Some(&error.to_string()),
                    None,
                ) {
                    log_error(&format!("수집 회차 {run_id} 실패 기록 실패"), &e);
                }
                RunSummary {
                    run_id,
                    status: RunStatus::Failed,
                    articles_new: 0,
                    matches_new: 0,
                }
            }
        }
    });
    Ok((run_id, done))
}

async fn collect<H, R, F, D>(
    deps: &CollectDeps<H, R, F, D>,
    run_id: i64,
    started_at: EpochMs,
    since: EpochMs,
) -> rusqlite::Result<RunSummary>
where
    H: HttpClient,
    R: RobotsCheck,
    F: FinnhubNewsClient,
    D: Decider,
{
    let (watched, resolver, backfill) = {
        let conn = deps.db.lock().expect("DB 잠금이 깨졌습니다");
        let watched = load_match_stocks(&conn)?;
        if watched.is_empty() {
            finish_run(
                &conn,
                run_id,
                deps.now(),
                RunStatus::Success,
                &[],
                0,
                0,
                Some("등록한 종목이 없어 수집하지 않았습니다."),
                None,
            )?;
            return Ok(RunSummary {
                run_id,
                status: RunStatus::Success,
                articles_new: 0,
                matches_new: 0,
            });
        }
        (
            watched,
            SinceResolver::load(&conn, started_at, since)?,
            load_backfill(&conn, started_at)?,
        )
    };
    if !backfill.is_empty() {
        log(&format!(
            "새로 등록한 종목 {}개는 등록 24시간 전 기사까지 연결합니다.",
            backfill.len()
        ));
    }
    let sources = build_sources(deps, &watched);
    let matcher = create_matcher(&watched);
    let display_names: HashMap<i64, String> = watched
        .iter()
        .map(|s| (s.id, s.name_ko.clone().unwrap_or_else(|| s.name.clone())))
        .collect();
    let ctx = RunCtx {
        deps,
        matcher: &matcher,
        display_names: &display_names,
        resolver: &resolver,
        run_since: since,
        backfill: &backfill,
        seen_at: started_at,
    };
    let results = map_limit(&sources, |source| run_source(&ctx, source)).await;

    {
        let conn = deps.db.lock().expect("DB 잠금이 깨졌습니다");
        if let Err(error) = prune_feed_headlines(&conn, deps.now()) {
            log_error("지난 피드 제목 지우기 실패", &error);
        }
    }

    let failed = results.iter().filter(|r| r.error.is_some()).count();
    let status = if failed == 0 {
        RunStatus::Success
    } else if failed == results.len() {
        RunStatus::Failed
    } else {
        RunStatus::Partial
    };
    let articles_new: i64 = results.iter().map(|r| r.saved).sum();
    let matches_new: i64 = results.iter().map(|r| r.matched).sum();
    // 회차가 돌았으면(완료·일부 실패) 새 종목의 지난 기사 연결은 끝난 것으로 본다. 모두 실패하면 다음 회차에 다시.
    if status != RunStatus::Failed {
        let conn = deps.db.lock().expect("DB 잠금이 깨졌습니다");
        clear_backfill(&conn, &backfill)?;
    }
    log(&format!(
        "수집 회차 {run_id}: {}, 소스 {}개(실패 {failed}), 새 기사 {articles_new}건, 새 연결 {matches_new}건",
        status.as_str(),
        results.len()
    ));

    // 수집 직후 새 연결(과 지난번에 못 한 것)을 평가한다(5단계 결정). 평가가 실패해도 수집 결과는 남긴다.
    let now_fn = deps
        .now
        .as_deref()
        .map(|f| f as &(dyn Fn() -> EpochMs + Send + Sync));
    let evaluation = match evaluate_pending(
        &deps.db,
        &EvaluateOptions {
            decider: deps.decider.as_ref(),
            now: now_fn,
        },
    )
    .await
    {
        Ok(evaluation) => {
            log(&format!(
                "평가 회차 {run_id}: {}건(기준 이상 {}, 실패 {}, 남김 {}), 성격 물음 {}·복사 {}·실패 {}, ${}{}",
                evaluation.evaluated,
                evaluation.relevant,
                evaluation.failed,
                evaluation.remaining,
                evaluation.nature_asked,
                evaluation.nature_copied,
                evaluation.nature_failed,
                js_to_fixed(evaluation.cost_usd, 5),
                evaluation
                    .note
                    .as_ref()
                    .map_or_else(String::new, |n| format!(", {n}"))
            ));
            Some(evaluation)
        }
        Err(error) => {
            log_error(&format!("평가 회차 {run_id} 실패"), &error);
            None
        }
    };
    {
        let conn = deps.db.lock().expect("DB 잠금이 깨졌습니다");
        finish_run(
            &conn,
            run_id,
            deps.now(),
            status,
            &results,
            articles_new,
            matches_new,
            None,
            evaluation.as_ref(),
        )?;
    }
    Ok(RunSummary {
        run_id,
        status,
        articles_new,
        matches_new,
    })
}

fn build_sources<H, R, F, D>(
    deps: &CollectDeps<H, R, F, D>,
    watched: &[MatchStock],
) -> Vec<Source> {
    let mut sources: Vec<Source> = deps
        .feeds
        .iter()
        .map(|feed| Source {
            id: feed.id.clone(),
            label: format!("{} {}", feed.publisher, feed.section),
            group: feed.publisher.clone(),
            keep_headlines: true,
            kind: SourceKind::Feed(feed.clone()),
        })
        .collect();
    if deps.finnhub.is_some() {
        // 시장·주제 항목 중 미국 기준인 것도 같은 조건에 걸린다(TS `watched.filter(s => s.market === "US")`).
        for stock in watched
            .iter()
            .filter(|s| s.market == crate::server::db::schema::Market::Us)
        {
            sources.push(Source {
                id: format!("finnhub:company:{}", stock.code),
                label: format!(
                    "Finnhub {}",
                    stock.name_ko.as_deref().unwrap_or(&stock.name)
                ),
                group: "Finnhub".to_string(),
                keep_headlines: false,
                kind: SourceKind::FinnhubCompany {
                    stock_id: stock.id,
                    code: stock.code.clone(),
                },
            });
        }
        sources.push(Source {
            id: "finnhub:general".to_string(),
            label: "Finnhub 시장 뉴스".to_string(),
            group: "Finnhub".to_string(),
            // 미국 종목별 뉴스는 한 종목으로 쏠리지 않게 남기지 않는다.
            keep_headlines: true,
            kind: SourceKind::FinnhubGeneral,
        });
    }
    sources
}

struct RunCtx<'a, H, R, F, D> {
    deps: &'a CollectDeps<H, R, F, D>,
    matcher: &'a Matcher,
    display_names: &'a HashMap<i64, String>,
    resolver: &'a SinceResolver,
    /// 이번 회차 기준 시각
    run_since: EpochMs,
    backfill: &'a Backfill,
    seen_at: EpochMs,
}

fn describe(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn finnhub_items(news: Vec<FinnhubNews>, direct: Vec<i64>) -> Vec<CollectedItem> {
    news.into_iter()
        .map(|n| CollectedItem {
            url: n.url,
            title: n.headline,
            summary: n.summary.unwrap_or_default(),
            publisher: n
                .source
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Finnhub".to_string()),
            published_at: Some((n.datetime * 1000.0) as i64),
            direct_stock_ids: direct.clone(),
        })
        .collect()
}

async fn fetch_source<H, R, F, D>(
    ctx: &RunCtx<'_, H, R, F, D>,
    source: &Source,
) -> Result<Vec<CollectedItem>, String>
where
    H: HttpClient,
    R: RobotsCheck,
    F: FinnhubNewsClient,
{
    let deps = ctx.deps;
    match &source.kind {
        SourceKind::Feed(feed) => {
            match deps.robots.check(&feed.url).await {
                RobotsVerdict::Allowed => {}
                RobotsVerdict::Disallowed => {
                    return Err("robots.txt가 막고 있어 건너뛰었습니다.".to_string());
                }
                RobotsVerdict::Unknown(cause) => {
                    return Err(format!(
                        "robots.txt를 확인하지 못해 건너뛰었습니다: {cause}"
                    ));
                }
            }
            let response = deps.http.get(&feed.url, &[]).await.map_err(describe)?;
            if response.status != 200 {
                return Err(format!("HTTP {}", response.status));
            }
            Ok(parse_feed(&response.text, ctx.seen_at)
                .into_iter()
                .map(|item| CollectedItem {
                    url: item.link,
                    title: item.title,
                    summary: item.summary,
                    publisher: feed.publisher.clone(),
                    published_at: item.published_at,
                    direct_stock_ids: Vec::new(),
                })
                .collect())
        }
        SourceKind::FinnhubCompany { stock_id, code } => {
            let Some(finnhub) = &deps.finnhub else {
                return Ok(Vec::new());
            };
            let since = ctx.resolver.since_for(&source.id);
            let from = ctx.backfill.get(stock_id).map(|b| b.from);
            let start = match from {
                Some(from) if from < since => from,
                _ => since,
            };
            let news = finnhub
                .company_news(code, start, ctx.seen_at)
                .await
                .map_err(describe)?;
            Ok(finnhub_items(news, vec![*stock_id]))
        }
        SourceKind::FinnhubGeneral => {
            let Some(finnhub) = &deps.finnhub else {
                return Ok(Vec::new());
            };
            let news = finnhub.general_news().await.map_err(describe)?;
            Ok(finnhub_items(news, Vec::new()))
        }
    }
}

async fn run_source<H, R, F, D>(ctx: &RunCtx<'_, H, R, F, D>, source: &Source) -> SourceResult
where
    H: HttpClient,
    R: RobotsCheck,
    F: FinnhubNewsClient,
{
    let started = Instant::now();
    let since = ctx.resolver.since_for(&source.id);
    let backfill_from = earliest(ctx.backfill);
    let mut result = SourceResult {
        id: source.id.clone(),
        label: source.label.clone(),
        group: source.group.clone(),
        fetched: 0,
        fresh: 0,
        saved: 0,
        matched: 0,
        since: if since != ctx.run_since {
            Some(since)
        } else {
            None
        },
        error: None,
        ms: 0,
    };
    let outcome: Result<(), String> = async {
        let items = fetch_source(ctx, source).await?;
        result.fetched = items.len() as i64;
        if source.keep_headlines {
            // 제목 보관이 실패해도 기사 수집·연결은 계속한다.
            let headlines: Vec<HeadlineItem> = items
                .iter()
                .map(|i| HeadlineItem {
                    title: i.title.clone(),
                    publisher: i.publisher.clone(),
                    published_at: i.published_at,
                    url: Some(i.url.clone()),
                    summary: Some(i.summary.clone()),
                })
                .collect();
            let mut conn = ctx.deps.db.lock().expect("DB 잠금이 깨졌습니다");
            if let Err(error) = save_feed_headlines(&mut conn, &source.id, &headlines, ctx.seen_at)
            {
                log_error(&format!("피드 제목 저장 실패: {}", source.id), &error);
            }
        }
        let mut keep: Vec<KeepItem> = Vec::new();
        for item in items {
            let Some(key) = url_key(&item.url) else {
                continue;
            };
            if js_trim(&item.title).is_empty() {
                continue;
            }
            // 발행 시각이 없는 피드(한겨레 등)나 미래 시각(국민일보 지면 기사)은 처음 받은 시각으로 본다.
            let published_at = effective_published_at(item.published_at, ctx.seen_at);
            let fresh = published_at >= since;
            if !fresh && !backfill_from.is_some_and(|from| published_at >= from) {
                continue;
            }
            if fresh {
                result.fresh += 1;
            }
            // 피드가 본문 전체를 요약 자리에 넣기도 해서, 저장하는 요약(앞 300자)까지만 보고 건다.
            let clipped = CollectedItem {
                summary: clip(&item.summary, SUMMARY_MAX_CHARS),
                ..item
            };
            let mut matches = merge_matches(&clipped, ctx.matcher, ctx.display_names);
            // 기준 시각 전 기사는 새로 등록한 종목에만 건다(D-20 보완).
            if !fresh {
                matches.retain(|m| {
                    ctx.backfill
                        .get(&m.stock_id)
                        .is_some_and(|b| published_at >= b.from)
                });
            }
            if !matches.is_empty() {
                keep.push(KeepItem {
                    item: clipped,
                    key,
                    published_at,
                    matches,
                });
            }
        }
        let mut conn = ctx.deps.db.lock().expect("DB 잠금이 깨졌습니다");
        let (articles, matches) =
            save_items(&mut conn, &source.id, &keep, ctx.seen_at).map_err(describe)?;
        result.saved = articles as i64;
        result.matched = matches as i64;
        Ok(())
    }
    .await;
    if let Err(error) = outcome {
        result.error = Some(error);
    }
    result.ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    result
}

/// 소스가 직접 지정한 종목은 종목 자체(0단계)로, 나머지는 제목+요약으로 찾는다.
fn merge_matches(
    item: &CollectedItem,
    matcher: &Matcher,
    display_names: &HashMap<i64, String>,
) -> Vec<StockMatch> {
    // JS `Map`처럼 처음 넣은 자리를 지키고 같은 종목은 값을 덮어쓴다.
    let mut order: Vec<StockMatch> = Vec::new();
    let mut index: HashMap<i64, usize> = HashMap::new();
    let mut put = |m: StockMatch| match index.get(&m.stock_id) {
        Some(&i) => order[i] = m,
        None => {
            index.insert(m.stock_id, order.len());
            order.push(m);
        }
    };
    for m in matcher.find(&item.title, &item.summary) {
        put(m);
    }
    for &stock_id in &item.direct_stock_ids {
        put(StockMatch {
            stock_id,
            keyword_path: display_names.get(&stock_id).cloned().unwrap_or_default(),
            keyword_reason: None,
            match_kind: MatchKind::Direct,
            level: 0,
            gated: false,
        });
    }
    order
}

/// 걸린 기사를 저장한다(같은 주소는 한 기사, D-19). 보관한 제목으로 지난 기사를 걸 때도 쓴다(11단계).
/// 돌려주는 값은 (새 기사 수, 새 연결 수)다.
pub fn save_items(
    conn: &mut Connection,
    feed_id: &str,
    keep: &[KeepItem],
    at: EpochMs,
) -> rusqlite::Result<(usize, usize)> {
    if keep.is_empty() {
        return Ok((0, 0));
    }
    let tx = conn.transaction()?;
    let (mut new_articles, mut new_matches) = (0, 0);
    for KeepItem {
        item,
        key,
        published_at,
        matches,
    } in keep
    {
        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM articles WHERE url_key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?;
        let article_id = match existing {
            Some(id) => id,
            None => {
                tx.execute(
                    "INSERT INTO articles (url_key, url, title, summary, publisher, feed, published_at, collected_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        key,
                        js_trim(&item.url),
                        clip(&item.title, TITLE_MAX_CHARS),
                        Some(item.summary.as_str()).filter(|s| !s.is_empty()),
                        item.publisher,
                        feed_id,
                        published_at,
                        at
                    ],
                )?;
                new_articles += 1;
                tx.last_insert_rowid()
            }
        };
        for m in matches {
            new_matches += tx.execute(
                "INSERT INTO article_matches (article_id, stock_id, keyword_path, keyword_reason, match_kind, level, gated, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) ON CONFLICT DO NOTHING",
                params![article_id, m.stock_id, m.keyword_path, m.keyword_reason, m.match_kind.as_str(), m.level, m.gated, at],
            )?;
        }
    }
    tx.commit()?;
    Ok((new_articles, new_matches))
}

/// JSON 글자 목록 칸. 깨졌으면 빈 목록
fn json_list(text: &str) -> Vec<String> {
    serde_json::from_str(text).unwrap_or_default()
}

/// 삭제하지 않은 종목과 키워드 트리
pub fn load_match_stocks(conn: &Connection) -> rusqlite::Result<Vec<MatchStock>> {
    struct Row {
        id: i64,
        kind: ItemKind,
        market: crate::server::db::schema::Market,
        code: String,
        name: String,
        name_ko: Option<String>,
        aliases: Vec<String>,
        anchors: Vec<String>,
    }
    let mut stmt = conn.prepare(
        "SELECT stocks.id, stocks.kind, stocks.market, stocks.code, stocks.name, stocks.name_ko, stocks.aliases, stocks.anchors, topics.status \
         FROM stocks LEFT JOIN topics ON topics.id = stocks.topic_id WHERE stocks.deleted_at IS NULL ORDER BY stocks.id ASC",
    )?;
    let rows: Vec<(Row, Option<String>)> = stmt
        .query_map([], |r| {
            let kind: String = r.get(1)?;
            let market: String = r.get(2)?;
            let bad = |index: usize, e: String| {
                rusqlite::Error::FromSqlConversionFailure(
                    index,
                    rusqlite::types::Type::Text,
                    e.into(),
                )
            };
            Ok((
                Row {
                    id: r.get(0)?,
                    kind: kind.parse().map_err(|e: String| bad(1, e))?,
                    market: market.parse().map_err(|e: String| bad(2, e))?,
                    code: r.get(3)?,
                    name: r.get(4)?,
                    name_ko: r.get(5)?,
                    aliases: json_list(&r.get::<_, String>(6)?),
                    anchors: json_list(&r.get::<_, String>(7)?),
                },
                r.get(8)?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    // 끝난 자동 토픽(마지막 갱신에서 빠진 것)의 항목에는 새 기사를 걸지 않는다(11단계).
    let active: Vec<Row> = rows
        .into_iter()
        .filter(|(_, status)| status.as_deref() != Some("ended"))
        .map(|(row, _)| row)
        .collect();
    if active.is_empty() {
        return Ok(Vec::new());
    }
    let marks = vec!["?"; active.len()].join(",");
    let mut kw_stmt = conn.prepare(&format!(
        "SELECT stock_id, level, text_ko, text_en, path, reason FROM keywords WHERE stock_id IN ({marks}) ORDER BY level ASC, id ASC"
    ))?;
    let mut by_stock: HashMap<i64, Vec<MatchKeyword>> = HashMap::new();
    let kw_rows = kw_stmt.query_map(
        rusqlite::params_from_iter(active.iter().map(|s| s.id)),
        |r| {
            Ok((
                r.get::<_, i64>(0)?,
                MatchKeyword {
                    level: r.get(1)?,
                    text_ko: r.get(2)?,
                    text_en: r.get(3)?,
                    path: r.get(4)?,
                    reason: r.get(5)?,
                },
            ))
        },
    )?;
    for row in kw_rows {
        let (stock_id, keyword) = row?;
        by_stock.entry(stock_id).or_default().push(keyword);
    }
    Ok(active
        .into_iter()
        .map(|s| MatchStock {
            id: s.id,
            kind: s.kind,
            market: s.market,
            code: s.code,
            name: s.name,
            name_ko: s.name_ko,
            aliases: s.aliases,
            anchors: s.anchors,
            keywords: by_stock.remove(&s.id).unwrap_or_default(),
        })
        .collect())
}

/// 이번 회차에서 볼 기사의 기준 시각(D-20).
/// 처음엔 최근 24시간, 이후엔 지난 회차(성공·일부 실패) 시작 1시간 전부터. 최대 7일.
pub fn compute_since(conn: &Connection, now: EpochMs) -> rusqlite::Result<EpochMs> {
    let last: Option<EpochMs> = conn
        .query_row(
            "SELECT started_at FROM collection_runs WHERE status IN ('success', 'partial') ORDER BY started_at DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    let floor = now - MAX_WINDOW_MS;
    let from = last.map_or(now - FIRST_RUN_WINDOW_MS, |started| started - OVERLAP_MS);
    Ok(from.max(floor))
}

/// 새로 등록한 종목: 종목 id → 이번 회차에서 연결할 가장 이른 발행 시각(from)과 DB에 적힌 값(stored)
pub struct BackfillEntry {
    pub from: EpochMs,
    pub stored: EpochMs,
}
pub type Backfill = HashMap<i64, BackfillEntry>;

/// 새로 등록하거나 되살린 종목(D-20 보완, 2026-10-21 결정): 이번 회차에서 등록 24시간 전 기사까지 연결한다.
/// 키워드를 만드는 중인 종목은 다음 회차로 미룬다(키워드로 걸 기사를 놓치지 않게). 최대 7일 전까지.
pub fn load_backfill(conn: &Connection, now: EpochMs) -> rusqlite::Result<Backfill> {
    let floor = now - MAX_WINDOW_MS;
    let mut stmt = conn.prepare(
        "SELECT id, backfill_from, keywords_status FROM stocks WHERE deleted_at IS NULL AND backfill_from IS NOT NULL",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, EpochMs>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    let mut backfill = Backfill::new();
    for row in rows {
        let (id, stored, status) = row?;
        if status == "pending" || status == "generating" {
            continue;
        }
        backfill.insert(
            id,
            BackfillEntry {
                from: stored.max(floor),
                stored,
            },
        );
    }
    Ok(backfill)
}

/// 이번 회차에서 지난 기사를 연결한 종목의 표시를 지운다. 그사이 다시 등록돼 값이 바뀐 종목은 남긴다.
fn clear_backfill(conn: &Connection, backfill: &Backfill) -> rusqlite::Result<()> {
    for (stock_id, entry) in backfill {
        conn.execute(
            "UPDATE stocks SET backfill_from = NULL WHERE id = ?1 AND backfill_from = ?2",
            params![stock_id, entry.stored],
        )?;
    }
    Ok(())
}

fn earliest(backfill: &Backfill) -> Option<EpochMs> {
    backfill.values().map(|e| e.from).min()
}

/// 소스마다 볼 기사의 기준 시각(D-24): 그 소스가 마지막으로 성공한 회차 시작 1시간 전.
/// 지난번에 실패한 소스는 이번 회차 기준 시각보다 앞에서부터 받아, 피드에 남은 기사를 놓치지 않는다.
/// 처음 보는 소스(새 피드·새로 등록한 미국 종목)는 회차 기준 시각, 최근 7일 동안 한 번도 성공하지 못한 소스는 7일 전부터.
pub struct SinceResolver {
    floor: EpochMs,
    run_since: EpochMs,
    last_ok: HashMap<String, EpochMs>,
    seen: HashSet<String>,
}

impl SinceResolver {
    pub fn load(conn: &Connection, now: EpochMs, run_since: EpochMs) -> rusqlite::Result<Self> {
        let floor = now - MAX_WINDOW_MS;
        let mut stmt = conn.prepare(
            "SELECT started_at, sources FROM collection_runs WHERE status IN ('success', 'partial', 'failed') \
             AND started_at >= ?1 ORDER BY started_at DESC",
        )?;
        let runs = stmt.query_map(params![floor], |r| {
            Ok((r.get::<_, EpochMs>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut last_ok: HashMap<String, EpochMs> = HashMap::new();
        let mut seen: HashSet<String> = HashSet::new();
        for run in runs {
            let (started_at, sources) = run?;
            let list: Vec<SourceResult> = serde_json::from_str::<Value>(&sources)
                .ok()
                .and_then(|v| {
                    v.as_array()
                        .map(|a| a.iter().filter_map(SourceResult::from_json).collect())
                })
                .unwrap_or_default();
            for source in list {
                seen.insert(source.id.clone());
                if source.error.is_none() && !last_ok.contains_key(&source.id) {
                    last_ok.insert(source.id, started_at);
                }
            }
        }
        Ok(Self {
            floor,
            run_since,
            last_ok,
            seen,
        })
    }

    pub fn since_for(&self, source_id: &str) -> EpochMs {
        let Some(ok) = self.last_ok.get(source_id) else {
            return if self.seen.contains(source_id) {
                self.floor
            } else {
                self.run_since
            };
        };
        let from = (ok - OVERLAP_MS).max(self.floor);
        from.min(self.run_since)
    }
}

/// `sourceSinceResolver(db, now, runSince)`와 같은 이름의 함수형 진입점
pub fn source_since_resolver(
    conn: &Connection,
    now: EpochMs,
    run_since: EpochMs,
) -> rusqlite::Result<SinceResolver> {
    SinceResolver::load(conn, now, run_since)
}

#[allow(clippy::too_many_arguments)]
fn finish_run(
    conn: &Connection,
    run_id: i64,
    at: EpochMs,
    status: RunStatus,
    sources: &[SourceResult],
    articles_new: i64,
    matches_new: i64,
    note: Option<&str>,
    evaluation: Option<&EvaluationSummary>,
) -> rusqlite::Result<()> {
    let sources_json =
        Value::Array(sources.iter().map(SourceResult::to_json).collect()).to_string();
    conn.execute(
        "UPDATE collection_runs SET status = ?1, finished_at = ?2, sources = ?3, articles_new = ?4, matches_new = ?5, \
         note = ?6, evaluation = ?7 WHERE id = ?8",
        params![
            status.as_str(),
            at,
            sources_json,
            articles_new,
            matches_new,
            note,
            evaluation.map(|e| e.to_json().to_string()),
            run_id
        ],
    )?;
    Ok(())
}

/// worker가 도중에 꺼져 '수집 중'으로 남은 회차를 실패로 정리한다(시작할 때 부른다).
pub fn close_stale_runs(conn: &Connection, now: EpochMs) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE collection_runs SET status = 'failed', finished_at = ?1, note = 'worker가 수집 도중 멈췄습니다.' \
         WHERE status = 'running'",
        params![now],
    )
}

/// 항목을 일꾼 6개가 나눠 처리한다(결과는 들어온 순서대로). 일꾼 수는 아래 `join!`의 개수다.
/// 동시에 처리하는 소스 수는 6이다(TS `SOURCE_CONCURRENCY`). 같은 사이트 간격은 HttpClient가 지킨다.
async fn map_limit<'a, T, R, Fut>(items: &'a [T], f: impl Fn(&'a T) -> Fut) -> Vec<R>
where
    Fut: std::future::Future<Output = R>,
{
    let results: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    let next = AtomicUsize::new(0);
    let worker = || async {
        loop {
            let index = next.fetch_add(1, Ordering::SeqCst);
            let Some(item) = items.get(index) else { return };
            let result = f(item).await;
            results.lock().expect("결과 잠금이 깨졌습니다")[index] = Some(result);
        }
    };
    tokio::join!(worker(), worker(), worker(), worker(), worker(), worker());
    results
        .into_inner()
        .expect("결과 잠금이 깨졌습니다")
        .into_iter()
        .map(|r| r.expect("모든 항목을 처리했다"))
        .collect()
}

fn clip(text: &str, max: usize) -> String {
    let trimmed = js_trim(text);
    if js_len(trimmed) > max {
        format!("{}…", js_slice(trimmed, 0, max - 1))
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
