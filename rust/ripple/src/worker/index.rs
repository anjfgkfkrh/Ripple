//! worker 시작 조립(`worker/index.ts`): DB를 열고, 클라이언트와 실행기를 만들고, 내부 API와 스케줄러를 띄운다.
//! 종료 신호(SIGTERM·SIGINT)를 받으면 스케줄러를 멈추고 0으로 끝난다.
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::server::ai::decider::typesafe::TypeSafeDecider;
use crate::server::ai::llm::models::{ModelListOptions, is_open_router_model};
use crate::server::ai::llm::openrouter::OpenRouterClient;
use crate::server::backup::run_backup;
use crate::server::collect::finnhub::FinnhubClient;
use crate::server::collect::http::{HttpClientOptions, ReqwestHttpClient};
use crate::server::collect::robots::RobotsChecker;
use crate::server::collect::run::{CollectDeps, close_stale_runs};
use crate::server::collect::runner::CollectionRunner;
use crate::server::db::client::{Db, DbAccess, enable_wal, into_db, open_db};
use crate::server::env::Env;
use crate::server::http_client::{collector_client, new_client};
use crate::server::keywords::generate::stocks_awaiting_keywords;
use crate::server::keywords::queue::KeywordQueue;
use crate::server::listings::sync::{
    ListingSources, MarketSyncResult, SyncOptions, sync_listings_if_stale,
};
use crate::server::log::{log, log_error};
use crate::server::prices::kis::{KisOptions, ReqwestKisClient};
use crate::server::prices::run::PriceRefresher;
use crate::server::schedule::scheduler::{
    BackupFn, BackupOutcome, BoxFuture, SchedulerDeps, start_scheduler,
};
use crate::server::time::now_ms;
use crate::server::topics::run::TopicRefresher;
use crate::server::topics::store::ensure_default_market;
use crate::worker::config::{WorkerConfig, read_worker_config};
use crate::worker::server::{CheckModelFn, WorkerDeps, worker_router};

type Http = Arc<ReqwestHttpClient>;
type Finnhub = Arc<FinnhubClient<Http>>;

/// worker를 실행하고 종료 코드를 돌려준다(시작 실패 1, 종료 신호로 끝나면 0).
pub fn run(env: &Env) -> i32 {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            log_error("worker 시작 실패", &error);
            return 1;
        }
    };
    // 단일 스레드 런타임: Node처럼 한 번에 한 작업만 돌고 `.await`에서만 다른 작업으로 넘어간다(설계 3.6).
    match runtime.block_on(start(env)) {
        Ok(()) => 0,
        Err(error) => {
            log_error("worker 시작 실패", &error);
            1
        }
    }
}

async fn start(env: &Env) -> Result<(), Box<dyn std::error::Error>> {
    let config = read_worker_config(env)?;

    let conn = open_db(&PathBuf::from(&config.database_path), DbAccess::ReadWrite)?;
    enable_wal(&conn)?;
    let db = into_db(conn);
    log(&format!(
        "DB 연결: {} (WAL, 쓰기 가능)",
        config.database_path
    ));

    if config.open_router_api_key.is_none() {
        log("OPENROUTER_API_KEY가 없어 AI 기능(종목 찾기 등)은 쓸 수 없습니다.");
    }
    let client = new_client();
    let llm = Arc::new(OpenRouterClient::new(
        config.open_router_api_key.clone(),
        Some(config.open_router_base_url.clone()),
        client.clone(),
    ));

    // 기사 평가와 토픽 평가(11단계)에 함께 쓴다. 키가 없으면 평가를 미룬다.
    let decider = config.typesafe_api_key.clone().map(|key| {
        Arc::new(TypeSafeDecider::new(
            Some(key),
            Some(config.typesafe_base_url.clone()),
            client.clone(),
        ))
    });
    let keyword_queue = Arc::new(KeywordQueue::new(db.clone(), llm.clone(), None));

    let stale = {
        let conn = db.lock().expect("DB 잠금이 깨졌습니다");
        close_stale_runs(&conn, now_ms())?
    };
    if stale > 0 {
        log(&format!(
            "도중에 멈춘 수집 회차 {stale}개를 실패로 정리했습니다."
        ));
    }
    if config.finnhub_api_key.is_none() {
        log("FINNHUB_API_KEY가 없어 미국 뉴스(Finnhub)는 수집하지 않습니다.");
    }
    if config.typesafe_api_key.is_none() {
        log("TYPESAFE_API_KEY가 없어 기사 평가(Jev)는 하지 않습니다.");
    }
    let http: Http = Arc::new(ReqwestHttpClient::new(
        collector_client(),
        HttpClientOptions::default(),
    ));
    let finnhub: Option<Finnhub> = config.finnhub_api_key.clone().map(|key| {
        Arc::new(FinnhubClient::new(
            http.clone(),
            key,
            Some(config.finnhub_base_url.clone()),
        ))
    });
    let collector = Arc::new(CollectionRunner::new(Arc::new(CollectDeps {
        db: db.clone(),
        http: http.clone(),
        robots: RobotsChecker::new(http.clone()),
        feeds: config.rss_feeds.clone(),
        finnhub: finnhub.clone(),
        decider: decider.clone(),
        now: None,
    })));

    // 종목 주가(13단계): 한국은 KIS, 미국은 Finnhub. 키가 없는 쪽은 받지 않는다.
    let kis = config.kis.as_ref().map(|kis| {
        Arc::new(ReqwestKisClient::new(KisOptions {
            app_key: kis.app_key.clone(),
            app_secret: kis.app_secret.clone(),
            env: kis.env,
            base_url: config.kis_base_url.clone(),
            now: None,
            min_interval_ms: None,
            timeout: None,
            http: Some(client.clone()),
        }))
    });
    match &config.kis {
        None => log("KIS_APP_KEY·KIS_APP_SECRET이 없어 한국 종목 주가는 받지 않습니다."),
        Some(kis) => log(&format!(
            "한국 종목 주가: 한국투자증권 KIS({} 키)",
            if kis.env == crate::server::prices::kis::KisEnv::Demo {
                "모의투자"
            } else {
                "실전"
            }
        )),
    }
    let price_refresher = Arc::new(PriceRefresher::new(db.clone(), kis, finnhub, None));

    // 자동 토픽 갱신(11단계): 정기 갱신과 "지금 갱신"이 같이 쓴다.
    let topic_refresher = TopicRefresher::new(db.clone(), llm.clone(), decider.clone(), None);

    let model_base_url = config.open_router_base_url.clone();
    let model_http = client.clone();
    let check_model: CheckModelFn = Arc::new(move |model_id| {
        let base_url = model_base_url.clone();
        let http = model_http.clone();
        Box::pin(async move {
            is_open_router_model(
                &http,
                &model_id,
                &ModelListOptions {
                    base_url: Some(base_url),
                    ..ModelListOptions::default()
                },
            )
            .await
        })
    });
    let app = worker_router(WorkerDeps {
        db: db.clone(),
        api_token: config.api_token.clone(),
        llm: llm.clone(),
        keyword_queue: keyword_queue.clone(),
        collector: collector.clone(),
        check_model,
        topic_refresher: Some(Arc::new(topic_refresher.clone())),
        now: None,
    });
    let listener = tokio::net::TcpListener::bind((config.host.as_str(), config.port)).await?;
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    log(&format!(
        "내부 API 대기 중: http://{}:{}",
        config.host, config.port
    ));

    // 한국 증시는 켠 채로 시작한다(11단계 결정). 한 번도 만든 적 없을 때만 만들고 키워드는 아래에서 만든다.
    let default_market = {
        let conn = db.lock().expect("DB 잠금이 깨졌습니다");
        ensure_default_market(&conn, now_ms())?
    };
    if default_market.is_some() {
        log("한국 증시 항목을 켰습니다(시장 전체 기사).");
    }
    // 대기 중이었거나 만들다 멈춘(재시작 등) 키워드 생성을 이어서 한다.
    let resumed = {
        let conn = db.lock().expect("DB 잠금이 깨졌습니다");
        stocks_awaiting_keywords(&conn)?
    };
    if !resumed.is_empty() {
        log(&format!(
            "키워드 생성 이어서 하기: {}개 종목",
            resumed.len()
        ));
    }
    for stock_id in resumed {
        keyword_queue.enqueue(stock_id);
    }

    // 정기 수집·키워드 자동 재생성·공식 목록 확인(6단계). 시작하자마자 한 번 확인해 놓친 일을 한다.
    if !config.scheduler_jobs_enabled {
        log("SCHEDULER_DISABLED: 정기 수집과 키워드 자동 재생성을 하지 않습니다.");
    }
    if config.backup_dir.is_none() {
        log("BACKUP_DIR가 없어 자동 백업을 하지 않습니다.");
    }
    // 이 값이 버려지면 타이머가 멈추므로 종료할 때까지 들고 있는다(R8 ⑬).
    let scheduler = start_scheduler(SchedulerDeps {
        db: db.clone(),
        collector: collector.clone(),
        keyword_queue: keyword_queue.clone(),
        sync_listings: create_listing_sync(
            db.clone(),
            config.listing_sources.clone(),
            client.clone(),
        ),
        backup: backup_function(&config),
        refresh_topics: Some(Arc::new(move || {
            let refresher = topic_refresher.clone();
            Box::pin(async move { refresher.refresh().await })
        })),
        refresh_prices: Some(Arc::new(move || {
            let refresher = price_refresher.clone();
            Box::pin(async move {
                refresher.refresh().await;
            })
        })),
        jobs_enabled: config.scheduler_jobs_enabled,
        now: None,
        tick_ms: None,
    });

    let signal = wait_for_signal().await;
    log(&format!("{signal} 수신, 종료합니다."));
    scheduler.stop();
    server.abort();
    Ok(())
}

/// 백업 폴더(BACKUP_DIR)가 있을 때만 백업 함수를 만든다. DB는 파일 경로로 따로 연다(R8 ⑥).
fn backup_function(config: &WorkerConfig) -> Option<BackupFn> {
    let dir = PathBuf::from(config.backup_dir.as_ref()?);
    let database_path = PathBuf::from(&config.database_path);
    Some(Arc::new(move |now| {
        let dir = dir.clone();
        let database_path = database_path.clone();
        Box::pin(async move {
            run_backup(database_path, dir, now)
                .await
                .map(|result| BackupOutcome {
                    file: result.file,
                    bytes: result.bytes,
                })
                .map_err(|error| error.to_string())
        })
    }))
}

/// 공식 종목 목록이 오래됐으면 받는다(D-12). 겹쳐 돌지 않고, 던지지 않는다.
fn create_listing_sync(
    db: Db,
    sources: ListingSources,
    http: reqwest::Client,
) -> Arc<dyn Fn() -> BoxFuture<()> + Send + Sync> {
    let running = Arc::new(AtomicBool::new(false));
    Arc::new(move || {
        let db = db.clone();
        let sources = sources.clone();
        let http = http.clone();
        let running = running.clone();
        Box::pin(async move {
            if running.swap(true, Ordering::SeqCst) {
                return;
            }
            let options = SyncOptions {
                sources,
                http,
                now: None,
            };
            match sync_listings_if_stale(&db, &options).await {
                Ok(results) => {
                    for result in results {
                        match result {
                            MarketSyncResult::Ok { market, count } => log(&format!(
                                "{} 공식 종목 목록 갱신: {count}개",
                                market.as_str()
                            )),
                            MarketSyncResult::Err { market, error } => log_error(
                                &format!(
                                    "{} 공식 종목 목록 갱신 실패(이전 목록 유지)",
                                    market.as_str()
                                ),
                                &error,
                            ),
                        }
                    }
                }
                Err(error) => log_error("공식 종목 목록 확인 실패", &error),
            }
            running.store(false, Ordering::SeqCst);
        })
    })
}

/// 종료 신호를 기다린다. 받은 신호 이름을 돌려준다.
async fn wait_for_signal() -> &'static str {
    use tokio::signal::unix::{SignalKind, signal};
    // 신호 처리를 등록하지 못하면 종료 신호를 받을 수 없으므로 시작하지 못한 것으로 본다.
    let mut term = signal(SignalKind::terminate()).expect("SIGTERM 처리를 등록하지 못했습니다");
    let mut int = signal(SignalKind::interrupt()).expect("SIGINT 처리를 등록하지 못했습니다");
    tokio::select! {
        _ = term.recv() => "SIGTERM",
        _ = int.recv() => "SIGINT",
    }
}
