use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::server::ai::decider::types::Decider;
use crate::server::ai::llm::LlmClient;
use crate::server::backup::backup_plan;
use crate::server::collect::finnhub::FinnhubNewsClient;
use crate::server::collect::http::HttpClient;
use crate::server::collect::robots::RobotsCheck;
use crate::server::collect::runner::CollectionRunner;
use crate::server::db::client::Db;
use crate::server::db::schema::RunTrigger;
use crate::server::keywords::queue::KeywordQueue;
use crate::server::log::{log, log_error};
use crate::server::settings::{
    BACKUP_LAST_AT, BACKUP_LAST_FILE, KEYWORDS_AUTO_REGENERATED_AT, write_num, write_str,
};
use crate::server::text::js_to_fixed;
use crate::server::time::{EpochMs, now_ms};
use crate::server::topics::plan::topic_plan;

use super::plan::{RegenerateEvery, collection_plan, keyword_plan};

/// 할 일을 확인하는 간격(D-31)
pub const SCHEDULER_TICK_MS: u64 = 60 * 1000;
/// 공식 종목 목록이 오래됐는지 확인하는 간격. 실제로 받는 것은 목록이 없거나 7일이 지났을 때뿐이다(D-12).
/// 처음 받기에 실패해도(네트워크 등) 30분 안에 다시 시도한다.
pub const LISTING_CHECK_INTERVAL_MS: i64 = 30 * 60 * 1000;
/// 백업이 실패하면(폴더 권한 등) 이만큼 뒤에 다시 시도한다. 1분마다 되풀이하지 않게.
pub const BACKUP_RETRY_MS: i64 = 60 * 60 * 1000;

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
/// 백업을 하는 함수(실패 이유는 글자로 돌려준다)
pub type BackupFn = Arc<dyn Fn(EpochMs) -> BoxFuture<Result<BackupOutcome, String>> + Send + Sync>;
type NowFn = Arc<dyn Fn() -> EpochMs + Send + Sync>;

/// 백업이 끝났을 때 알려 주는 값
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupOutcome {
    pub file: String,
    pub bytes: u64,
}

/// 수집을 시작하는 쪽(`CollectionRunner`, 테스트의 가짜). 이미 도는 중이거나 시작하지 못하면 None
pub trait CollectStarter: Send + Sync {
    fn start(&self, trigger: RunTrigger) -> Option<i64>;
}

impl<H, R, F, D> CollectStarter for CollectionRunner<H, R, F, D>
where
    H: HttpClient + 'static,
    R: RobotsCheck + 'static,
    F: FinnhubNewsClient + 'static,
    D: Decider + 'static,
{
    fn start(&self, trigger: RunTrigger) -> Option<i64> {
        CollectionRunner::start(self, trigger)
    }
}

/// 키워드 생성 대기열에 넣는 쪽(`KeywordQueue`, 테스트의 가짜)
pub trait KeywordEnqueuer: Send + Sync {
    /// 전체 생성으로 넣는다(등록·"다시 만들기")
    fn enqueue(&self, stock_id: i64);
    /// 정기 점검으로 넣는다. 구현이 따로 두지 않으면 전체 생성과 같다.
    fn enqueue_periodic(&self, stock_id: i64) {
        self.enqueue(stock_id);
    }
}

impl<L> KeywordEnqueuer for KeywordQueue<L>
where
    L: LlmClient + 'static,
{
    fn enqueue(&self, stock_id: i64) {
        KeywordQueue::enqueue(self, stock_id)
    }

    fn enqueue_periodic(&self, stock_id: i64) {
        KeywordQueue::enqueue_periodic(self, stock_id)
    }
}

pub struct SchedulerDeps<C, K> {
    pub db: Db,
    pub collector: Arc<C>,
    pub keyword_queue: Arc<K>,
    /// 공식 종목 목록이 오래됐으면 받는다. 실패는 스스로 처리한다.
    pub sync_listings: Arc<dyn Fn() -> BoxFuture<()> + Send + Sync>,
    /// DB를 백업한다(D-27). 백업 폴더(BACKUP_DIR)를 정하지 않았으면 없다. 실패 이유는 글자로 돌려준다.
    pub backup: Option<BackupFn>,
    /// 자동 토픽을 갱신한다(11단계). 성공하면 true. 실패는 스스로 처리한다.
    pub refresh_topics: Option<Arc<dyn Fn() -> BoxFuture<bool> + Send + Sync>>,
    /// 받을 때가 된 종목 주가를 받는다(13단계, 1분마다). 실패는 스스로 처리한다.
    pub refresh_prices: Option<Arc<dyn Fn() -> BoxFuture<()> + Send + Sync>>,
    /// false면 정기 수집·키워드 자동 재생성을 하지 않는다(E2E용). 목록 확인은 한다.
    pub jobs_enabled: bool,
    pub now: Option<NowFn>,
    /// 확인 간격(ms). 없으면 1분
    pub tick_ms: Option<u64>,
}

struct Inner<C, K> {
    deps: SchedulerDeps<C, K>,
    now: NowFn,
    /// 마지막으로 목록을 확인한 시각(없으면 None)
    last_listing_check: Mutex<Option<EpochMs>>,
    backup_running: Arc<AtomicBool>,
    /// 이 시각 전에는 백업을 다시 시도하지 않는다(없으면 None)
    backup_retry_at: Mutex<Option<EpochMs>>,
    topics_running: Arc<AtomicBool>,
    prices_running: Arc<AtomicBool>,
    /// 도는 백그라운드 작업 수와 끝났다는 알림(테스트·종료용 `idle`)
    active: AtomicUsize,
    notify: Notify,
}

/// 켜진 동안 `active`를 하나 올리고, 끝나면(패닉 포함) 내리며 `flag`를 내린다(JS `.finally`).
struct TaskGuard<C, K> {
    inner: Arc<Inner<C, K>>,
    flag: Option<Arc<AtomicBool>>,
}

impl<C, K> TaskGuard<C, K> {
    fn new(inner: &Arc<Inner<C, K>>, flag: Option<Arc<AtomicBool>>) -> Self {
        inner.active.fetch_add(1, Ordering::SeqCst);
        Self {
            inner: inner.clone(),
            flag,
        }
    }
}

impl<C, K> Drop for TaskGuard<C, K> {
    fn drop(&mut self) {
        if let Some(flag) = &self.flag {
            flag.store(false, Ordering::SeqCst);
        }
        self.inner.active.fetch_sub(1, Ordering::SeqCst);
        self.inner.notify.notify_waiters();
    }
}

/// worker의 주기 작업(SPEC FR-6, D-31). 라이브러리 없이 1분마다 DB를 보고 할 일을 시작한다.
/// 시계(DB의 마지막 실행 시각)를 비교하므로 worker를 다시 켰을 때와 노트북이 절전에서 깨어났을 때
/// 놓친 일을 같은 방식으로 한 번 한다. 수집과 키워드 재생성은 서로 기다리지 않는다(2026-10-21 결정).
/// tokio 런타임 안에서만 쓴다.
pub struct Scheduler<C, K> {
    inner: Arc<Inner<C, K>>,
    timer: JoinHandle<()>,
}

const TASK_LOCK: &str = "스케줄러 잠금이 깨졌습니다";

pub fn start_scheduler<C, K>(deps: SchedulerDeps<C, K>) -> Scheduler<C, K>
where
    C: CollectStarter + 'static,
    K: KeywordEnqueuer + 'static,
{
    let tick_ms = deps.tick_ms.unwrap_or(SCHEDULER_TICK_MS);
    let now: NowFn = deps.now.clone().unwrap_or_else(|| Arc::new(now_ms));
    let inner = Arc::new(Inner {
        deps,
        now,
        last_listing_check: Mutex::new(None),
        backup_running: Arc::new(AtomicBool::new(false)),
        backup_retry_at: Mutex::new(None),
        topics_running: Arc::new(AtomicBool::new(false)),
        prices_running: Arc::new(AtomicBool::new(false)),
        active: AtomicUsize::new(0),
        notify: Notify::new(),
    });
    tick(&inner);
    let timer_inner = inner.clone();
    let period = Duration::from_millis(tick_ms);
    let timer = tokio::spawn(async move {
        // 처음 확인은 위에서 했으니 한 주기 뒤부터. 밀린 틱은 몰아서 하지 않고 늦춘다(setInterval과 비슷).
        let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            tick(&timer_inner);
        }
    });
    Scheduler { inner, timer }
}

impl<C, K> Scheduler<C, K>
where
    C: CollectStarter + 'static,
    K: KeywordEnqueuer + 'static,
{
    /// 할 일을 한 번 확인한다(시작할 때와 1분마다). 패닉이 아니면 오류를 밖으로 내지 않는다.
    pub fn tick(&self) {
        tick(&self.inner);
    }

    pub fn stop(&self) {
        self.timer.abort();
    }

    /// 시작해 둔 백그라운드 작업(백업·토픽·주가·목록)이 모두 끝날 때까지 기다린다(테스트·종료용).
    pub async fn idle(&self) {
        loop {
            let notified = self.inner.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.inner.active.load(Ordering::SeqCst) == 0 {
                return;
            }
            notified.await;
        }
    }
}

impl<C, K> Drop for Scheduler<C, K> {
    fn drop(&mut self) {
        self.timer.abort();
    }
}

fn trigger_label(trigger: RunTrigger) -> &'static str {
    match trigger {
        RunTrigger::CatchUp => "누락 보정",
        _ => "정기",
    }
}

fn tick<C, K>(inner: &Arc<Inner<C, K>>)
where
    C: CollectStarter + 'static,
    K: KeywordEnqueuer + 'static,
{
    let deps = &inner.deps;
    let at = (inner.now)();
    if deps.jobs_enabled {
        // DB 잠금은 계획을 읽는 동안만 쥔다. 수집·대기열은 스스로 DB를 잠그므로 쥔 채로 부르지 않는다.
        match with_conn(&deps.db, |conn| collection_plan(conn, at)) {
            Ok(plan) => {
                if plan.due
                    && let Some(run_id) = deps.collector.start(plan.trigger)
                {
                    let label = if plan.retrying {
                        "실패 뒤 다시"
                    } else {
                        trigger_label(plan.trigger)
                    };
                    log(&format!("수집 시작: 회차 {run_id}({label})"));
                }
            }
            Err(error) => log_error("정기 수집 확인 실패", &error),
        }
        match with_conn(&deps.db, |conn| keyword_plan(conn, at)) {
            Ok(plan) => {
                if plan.due {
                    for stock_id in &plan.stock_ids {
                        deps.keyword_queue.enqueue_periodic(*stock_id);
                    }
                    let written = with_conn(&deps.db, |conn| {
                        write_num(conn, &KEYWORDS_AUTO_REGENERATED_AT, at as f64, at)
                    });
                    match written {
                        Ok(()) => log(&format!(
                            "키워드 정기 점검(매일 06:00 확인, 주기 {}): {}개 종목",
                            if plan.every == RegenerateEvery::Weekly {
                                "매주 월요일"
                            } else {
                                "매일"
                            },
                            plan.stock_ids.len()
                        )),
                        Err(error) => log_error("키워드 자동 재생성 확인 실패", &error),
                    }
                }
            }
            Err(error) => log_error("키워드 자동 재생성 확인 실패", &error),
        }
        // 자동 토픽 갱신(11단계): 간격·특정 시각이 되면 한 번. 실패하면 30분 뒤 다시(계획이 DB 기록으로 판단).
        if let Some(refresh_topics) = &deps.refresh_topics
            && !inner.topics_running.load(Ordering::SeqCst)
        {
            match with_conn(&deps.db, |conn| topic_plan(conn, at)) {
                Ok(plan) => {
                    if plan.due {
                        inner.topics_running.store(true, Ordering::SeqCst);
                        let guard = TaskGuard::new(inner, Some(inner.topics_running.clone()));
                        let future = refresh_topics();
                        tokio::spawn(async move {
                            let _guard = guard;
                            future.await;
                        });
                    }
                }
                Err(error) => log_error("토픽 갱신 확인 실패", &error),
            }
        }
        // 주가(13단계): 1분마다. 장중인지는 refreshPrices가 종목마다 본다. 앞 회차가 길어지면 건너뛴다.
        if let Some(refresh_prices) = &deps.refresh_prices
            && !inner.prices_running.load(Ordering::SeqCst)
        {
            inner.prices_running.store(true, Ordering::SeqCst);
            let guard = TaskGuard::new(inner, Some(inner.prices_running.clone()));
            let future = refresh_prices();
            tokio::spawn(async move {
                let _guard = guard;
                future.await;
            });
        }
    }
    // 백업은 정기 작업을 꺼도(E2E) 폴더가 정해져 있으면 한다(D-27: 매일 04:00, 놓쳤으면 켜질 때).
    if let Some(backup) = &deps.backup {
        let retry_at = *inner.backup_retry_at.lock().expect(TASK_LOCK);
        if !inner.backup_running.load(Ordering::SeqCst) && retry_at.is_none_or(|retry| at >= retry)
        {
            match with_conn(&deps.db, |conn| backup_plan(conn, at)) {
                Ok(plan) => {
                    if plan.due {
                        inner.backup_running.store(true, Ordering::SeqCst);
                        let guard = TaskGuard::new(inner, Some(inner.backup_running.clone()));
                        let future = backup(at);
                        let task_inner = inner.clone();
                        tokio::spawn(async move {
                            let _guard = guard;
                            // 기록에 실패해도 백업 실패처럼 1시간 뒤 다시 한다(TS는 `.then`의 예외가 `.catch`로 간다).
                            let result = match future.await {
                                Ok(outcome) => record_backup(&task_inner, at, &outcome),
                                Err(error) => Err(error),
                            };
                            if let Err(error) = result {
                                *task_inner.backup_retry_at.lock().expect(TASK_LOCK) =
                                    Some(at + BACKUP_RETRY_MS);
                                log_error("백업 실패(1시간 뒤 다시 시도)", &error);
                            }
                        });
                    }
                }
                Err(error) => log_error("백업 확인 실패", &error),
            }
        }
    }
    let list_due = {
        let mut last = inner.last_listing_check.lock().expect(TASK_LOCK);
        let due = last.is_none_or(|checked| at - checked >= LISTING_CHECK_INTERVAL_MS);
        if due {
            *last = Some(at);
        }
        due
    };
    if list_due {
        let guard = TaskGuard::new(inner, None);
        let future = (deps.sync_listings)();
        tokio::spawn(async move {
            let _guard = guard;
            future.await;
        });
    }
}

/// 백업이 끝난 것을 설정에 적고 로그에 남긴다.
fn record_backup<C, K>(
    inner: &Arc<Inner<C, K>>,
    at: EpochMs,
    outcome: &BackupOutcome,
) -> Result<(), String> {
    with_conn(&inner.deps.db, |conn| {
        write_num(conn, &BACKUP_LAST_AT, at as f64, at)?;
        write_str(conn, &BACKUP_LAST_FILE, &outcome.file, at)
    })
    .map_err(|error| error.to_string())?;
    log(&format!(
        "백업: {} ({}MB)",
        outcome.file,
        js_to_fixed(outcome.bytes as f64 / 1024.0 / 1024.0, 1)
    ));
    Ok(())
}

fn with_conn<T>(
    db: &Db,
    body: impl FnOnce(&rusqlite::Connection) -> rusqlite::Result<T>,
) -> rusqlite::Result<T> {
    let conn = db.lock().expect("DB 잠금이 깨졌습니다");
    body(&conn)
}

#[cfg(test)]
#[path = "scheduler_tests.rs"]
mod tests;
