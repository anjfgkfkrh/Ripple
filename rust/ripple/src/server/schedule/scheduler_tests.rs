use std::sync::atomic::AtomicI64;

use rusqlite::params;
use tokio::sync::oneshot;

use super::*;
use crate::server::settings::{
    TOPICS_FAILED_AT, TOPICS_INTERVAL_HOURS, TOPICS_REFRESHED_AT, TOPICS_STARTED_AT, read_num,
    read_str,
};
use crate::server::stocks::repository::{RegisterInput, RegisterResult, register_stock};
use crate::test_support::{SharedTestDb, ms, seed_listings, shared_test_db};

// 한국 시간 2026-10-21(수) 12:18
fn start_time() -> EpochMs {
    ms("2026-10-21T03:18:00Z")
}
const MIN: i64 = 60 * 1000;

#[derive(Default)]
struct FakeCollector {
    started: Mutex<Vec<RunTrigger>>,
    /// true면 시작하지 못한 것으로 None을 돌려준다.
    refuse: AtomicBool,
}

impl CollectStarter for FakeCollector {
    fn start(&self, trigger: RunTrigger) -> Option<i64> {
        if self.refuse.load(Ordering::SeqCst) {
            return None;
        }
        let mut started = self.started.lock().unwrap();
        started.push(trigger);
        Some(started.len() as i64)
    }
}

#[derive(Default)]
struct FakeKeywordQueue {
    enqueued: Mutex<Vec<i64>>,
}

impl KeywordEnqueuer for FakeKeywordQueue {
    fn enqueue(&self, stock_id: i64) {
        self.enqueued.lock().unwrap().push(stock_id);
    }
}

type Clock = Arc<AtomicI64>;
type TestScheduler = Scheduler<FakeCollector, FakeKeywordQueue>;

struct Setup {
    shared: SharedTestDb,
    stock_id: i64,
    collector: Arc<FakeCollector>,
    keyword_queue: Arc<FakeKeywordQueue>,
    listing_checks: Arc<Mutex<Vec<EpochMs>>>,
    clock: Clock,
}

#[derive(Default)]
struct Options {
    jobs_disabled: bool,
    backup: Option<BackupFn>,
    refresh_topics: Option<Arc<dyn Fn() -> BoxFuture<bool> + Send + Sync>>,
    refresh_prices: Option<Arc<dyn Fn() -> BoxFuture<()> + Send + Sync>>,
}

impl Setup {
    fn new() -> Self {
        let shared = shared_test_db();
        let stock_id = {
            let mut conn = shared.db.lock().unwrap();
            seed_listings(&mut conn);
            let id = match register_stock(
                &conn,
                &RegisterInput {
                    market: crate::server::db::schema::Market::Kr,
                    code: "005930",
                    user_input: "삼전",
                    name_ko: None,
                },
                start_time(),
            )
            .unwrap()
            {
                RegisterResult::Created(stock) => stock.id,
                other => panic!("표본에 없음: {other:?}"),
            };
            // 이번 주기(06:00) 전에 만든 키워드
            conn.execute(
                "UPDATE stocks SET keywords_status = 'ready', keywords_generated_at = ?1 WHERE id = ?2",
                params![ms("2026-10-20T10:00:00Z"), id],
            )
            .unwrap();
            id
        };
        Self {
            shared,
            stock_id,
            collector: Arc::new(FakeCollector::default()),
            keyword_queue: Arc::new(FakeKeywordQueue::default()),
            listing_checks: Arc::new(Mutex::new(Vec::new())),
            clock: Arc::new(AtomicI64::new(start_time())),
        }
    }

    fn advance(&self, minutes: i64) {
        self.clock.fetch_add(minutes * MIN, Ordering::SeqCst);
    }

    fn start(&self, options: Options) -> TestScheduler {
        self.start_with(options, self.collector.clone())
    }

    fn start_with(&self, options: Options, collector: Arc<FakeCollector>) -> TestScheduler {
        let (checks, clock) = (self.listing_checks.clone(), self.clock.clone());
        start_scheduler(SchedulerDeps {
            db: self.shared.db.clone(),
            collector,
            keyword_queue: self.keyword_queue.clone(),
            sync_listings: Arc::new(move || {
                // TS는 async 함수 본문이 바로 실행되므로 기록도 부른 즉시 한다.
                checks.lock().unwrap().push(clock.load(Ordering::SeqCst));
                Box::pin(async {})
            }),
            backup: options.backup,
            refresh_topics: options.refresh_topics,
            refresh_prices: options.refresh_prices,
            jobs_enabled: !options.jobs_disabled,
            now: Some({
                let clock = self.clock.clone();
                Arc::new(move || clock.load(Ordering::SeqCst))
            }),
            // 테스트에서는 tick()을 직접 부른다.
            tick_ms: Some(24 * 60 * 60 * 1000),
        })
    }

    fn setting_num(&self, setting: &crate::server::settings::NumSetting) -> f64 {
        read_num(&self.shared.db.lock().unwrap(), setting).unwrap()
    }

    fn insert_run(&self, trigger: &str, status: &str, started_at: EpochMs) {
        self.shared
            .db
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO collection_runs (trigger, status, started_at, since) VALUES (?1, ?2, ?3, ?4)",
                params![trigger, status, started_at, start_time()],
            )
            .unwrap();
    }
}

fn started(collector: &FakeCollector) -> Vec<RunTrigger> {
    collector.started.lock().unwrap().clone()
}

fn enqueued(queue: &FakeKeywordQueue) -> Vec<i64> {
    queue.enqueued.lock().unwrap().clone()
}

// describe("startScheduler (SPEC FR-6, D-31)")
#[tokio::test(flavor = "current_thread")]
async fn at_start_catches_up_missed_collection_keyword_regeneration_and_listing_check() {
    // it("시작하자마자 놓친 수집을 누락 보정으로 하고, 놓친 키워드 재생성과 목록 확인도 한다")
    let t = Setup::new();
    t.insert_run("scheduled", "success", start_time() - 120 * MIN);

    let scheduler = t.start(Options::default());

    assert_eq!(started(&t.collector), [RunTrigger::CatchUp]);
    assert_eq!(enqueued(&t.keyword_queue), [t.stock_id]);
    assert_eq!(
        t.setting_num(&KEYWORDS_AUTO_REGENERATED_AT),
        start_time() as f64
    );
    assert_eq!(*t.listing_checks.lock().unwrap(), [start_time()]);
    scheduler.idle().await;
}

#[tokio::test(flavor = "current_thread")]
async fn does_not_start_before_the_interval_and_starts_a_regular_run_after() {
    // it("간격이 지나기 전에는 시작하지 않고, 지나면 정기 수집을 시작한다")
    let t = Setup::new();
    t.insert_run("manual", "success", start_time() - 10 * MIN);
    let scheduler = t.start(Options::default());
    assert!(started(&t.collector).is_empty());

    t.advance(19);
    scheduler.tick();
    assert!(started(&t.collector).is_empty());
    t.advance(1);
    scheduler.tick();
    assert_eq!(started(&t.collector), [RunTrigger::Scheduled]);
}

#[tokio::test(flavor = "current_thread")]
async fn keyword_regeneration_runs_once_per_cycle() {
    // it("키워드 자동 재생성은 주기마다 한 번만 한다")
    let t = Setup::new();
    let scheduler = t.start(Options::default());
    t.advance(1);
    scheduler.tick();
    assert_eq!(enqueued(&t.keyword_queue), [t.stock_id]);
}

#[tokio::test(flavor = "current_thread")]
async fn listing_check_every_thirty_minutes() {
    // it("공식 목록 확인은 30분마다")
    let t = Setup::new();
    let scheduler = t.start(Options::default());
    t.advance(29);
    scheduler.tick();
    assert_eq!(t.listing_checks.lock().unwrap().len(), 1);
    t.advance(1);
    scheduler.tick();
    assert_eq!(t.listing_checks.lock().unwrap().len(), 2);
}

#[tokio::test(flavor = "current_thread")]
async fn turning_jobs_off_leaves_only_the_listing_check() {
    // it("정기 작업을 끄면(E2E) 수집·키워드는 하지 않고 목록 확인만 한다")
    let t = Setup::new();
    let _scheduler = t.start(Options {
        jobs_disabled: true,
        ..Options::default()
    });
    assert!(started(&t.collector).is_empty());
    assert!(enqueued(&t.keyword_queue).is_empty());
    assert_eq!(t.listing_checks.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn a_refused_collection_start_does_not_stop_keywords_and_listing_check() {
    // it("수집 시작이 실패해도 키워드 재생성과 목록 확인은 한다")
    // Rust의 수집 실행기는 시작 실패를 던지지 않고 None으로 돌려주므로(로그는 실행기가 남김) 그 경우로 옮겼다.
    let t = Setup::new();
    let collector = Arc::new(FakeCollector::default());
    collector.refuse.store(true, Ordering::SeqCst);
    let _scheduler = t.start_with(Options::default(), collector.clone());
    assert!(started(&collector).is_empty());
    assert_eq!(enqueued(&t.keyword_queue), [t.stock_id]);
    assert_eq!(t.listing_checks.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn a_failing_plan_read_does_not_stop_the_other_jobs() {
    // Rust 전용: TS의 try/catch 칸막이 — 수집 계획을 읽지 못해도(표가 없음) 나머지 작업은 한다.
    let t = Setup::new();
    t.shared
        .db
        .lock()
        .unwrap()
        .execute_batch("ALTER TABLE collection_runs RENAME TO collection_runs_gone")
        .unwrap();
    let _scheduler = t.start(Options::default());
    assert!(started(&t.collector).is_empty());
    assert_eq!(enqueued(&t.keyword_queue), [t.stock_id]);
    assert_eq!(t.listing_checks.lock().unwrap().len(), 1);
}

fn outcome_backup(
    calls: Arc<Mutex<Vec<EpochMs>>>,
    result: Result<BackupOutcome, String>,
) -> BackupFn {
    Arc::new(move |at| {
        calls.lock().unwrap().push(at);
        let result = result.clone();
        Box::pin(async move { result })
    })
}

#[tokio::test(flavor = "current_thread")]
async fn backs_up_once_per_four_oclock_cycle_and_records_it_even_with_jobs_off() {
    // it("백업 폴더가 있으면 04:00 주기마다 한 번 백업하고 기록한다(정기 작업을 꺼도)")
    let t = Setup::new();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let scheduler = t.start(Options {
        jobs_disabled: true,
        backup: Some(outcome_backup(
            calls.clone(),
            Ok(BackupOutcome {
                file: "ripple-20261021-121800.db".into(),
                bytes: 1024,
            }),
        )),
        ..Options::default()
    });
    scheduler.idle().await;
    assert_eq!(*calls.lock().unwrap(), [start_time()]);
    assert_eq!(t.setting_num(&BACKUP_LAST_AT), start_time() as f64);
    assert_eq!(
        read_str(&t.shared.db.lock().unwrap(), &BACKUP_LAST_FILE).unwrap(),
        "ripple-20261021-121800.db"
    );
    t.advance(60);
    scheduler.tick();
    scheduler.idle().await;
    assert_eq!(calls.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn failed_backup_retries_after_one_hour_not_every_minute() {
    // it("백업이 실패하면 1분마다 되풀이하지 않고 1시간 뒤에 다시 한다")
    let t = Setup::new();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let scheduler = t.start(Options {
        backup: Some(outcome_backup(
            calls.clone(),
            Err("폴더에 쓸 수 없음".into()),
        )),
        ..Options::default()
    });
    scheduler.idle().await;
    t.advance(30);
    scheduler.tick();
    scheduler.idle().await;
    assert_eq!(calls.lock().unwrap().len(), 1);
    t.advance(31);
    scheduler.tick();
    scheduler.idle().await;
    assert_eq!(calls.lock().unwrap().len(), 2);
    assert_eq!(t.setting_num(&BACKUP_LAST_AT), 0.0);
}

#[tokio::test(flavor = "current_thread")]
async fn a_backup_that_cannot_be_recorded_is_retried_like_a_failed_one() {
    // TS의 `.then(...)` 안에서 기록이 던지면 `.catch`로 가서 1시간 뒤 다시 한다.
    let t = Setup::new();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let db = t.shared.db.clone();
    let backup = {
        let calls = calls.clone();
        Arc::new(
            move |at: EpochMs| -> BoxFuture<Result<BackupOutcome, String>> {
                calls.lock().unwrap().push(at);
                // 백업은 되었지만 설정 표를 쓸 수 없게 만든다.
                db.lock()
                    .unwrap()
                    .execute_batch("ALTER TABLE settings RENAME TO settings_gone")
                    .ok();
                Box::pin(async {
                    Ok(BackupOutcome {
                        file: "ripple-x.db".into(),
                        bytes: 1,
                    })
                })
            },
        )
    };
    let scheduler = t.start(Options {
        jobs_disabled: true,
        backup: Some(backup),
        ..Options::default()
    });
    scheduler.idle().await;
    assert_eq!(calls.lock().unwrap().len(), 1);
    t.advance(30);
    scheduler.tick();
    scheduler.idle().await;
    assert_eq!(calls.lock().unwrap().len(), 1);
}

/// 끝낼 때까지 멈춰 있는 갱신을 만든다. 부른 즉시 `calls`에 시각을 남긴다(TS 시험의 모양).
struct HeldTopics {
    calls: Arc<Mutex<Vec<EpochMs>>>,
    ok: Arc<AtomicBool>,
    finish: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}

impl HeldTopics {
    fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            ok: Arc::new(AtomicBool::new(false)),
            finish: Arc::new(Mutex::new(None)),
        }
    }

    fn refresh_fn(&self, t: &Setup) -> Arc<dyn Fn() -> BoxFuture<bool> + Send + Sync> {
        let (calls, ok, finish) = (self.calls.clone(), self.ok.clone(), self.finish.clone());
        let (db, clock) = (t.shared.db.clone(), t.clock.clone());
        Arc::new(move || {
            let at = clock.load(Ordering::SeqCst);
            calls.lock().unwrap().push(at);
            // 실제 갱신처럼 시작·끝(성공 또는 실패)을 설정에 남긴다.
            write_num(&db.lock().unwrap(), &TOPICS_STARTED_AT, at as f64, at).unwrap();
            let (sender, receiver) = oneshot::channel();
            *finish.lock().unwrap() = Some(sender);
            let (db, ok) = (db.clone(), ok.clone());
            Box::pin(async move {
                receiver.await.unwrap();
                let ok = ok.load(Ordering::SeqCst);
                let setting = if ok {
                    &TOPICS_REFRESHED_AT
                } else {
                    &TOPICS_FAILED_AT
                };
                write_num(&db.lock().unwrap(), setting, at as f64, at).unwrap();
                ok
            })
        })
    }

    fn finish(&self) {
        self.finish
            .lock()
            .unwrap()
            .take()
            .expect("끝낼 갱신이 없다")
            .send(())
            .unwrap();
    }

    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

#[tokio::test(flavor = "current_thread")]
async fn topic_refresh_runs_first_time_then_by_interval_retries_in_thirty_minutes_without_overlap()
{
    // it("토픽 갱신(11단계): 처음이면 바로, 그 뒤에는 간격마다. 실패하면 30분 뒤에 다시 하고, 도는 동안은 겹쳐 시작하지 않는다")
    let t = Setup::new();
    let held = HeldTopics::new();
    let scheduler = t.start(Options {
        refresh_topics: Some(held.refresh_fn(&t)),
        ..Options::default()
    });
    assert_eq!(*held.calls.lock().unwrap(), [start_time()]);
    // 도는 동안에는 다시 시작하지 않는다.
    t.advance(1);
    scheduler.tick();
    assert_eq!(held.call_count(), 1);
    held.finish();
    scheduler.idle().await;

    // 실패: 30분이 지나야 다시 한다.
    t.advance(28);
    scheduler.tick();
    assert_eq!(held.call_count(), 1);
    t.advance(1);
    scheduler.tick();
    assert_eq!(held.call_count(), 2);
    held.ok.store(true, Ordering::SeqCst);
    held.finish();
    scheduler.idle().await;

    // 성공 뒤에는 간격(기본 3시간)마다
    t.advance(179);
    scheduler.tick();
    assert_eq!(held.call_count(), 2);
    t.advance(1);
    scheduler.tick();
    assert_eq!(held.call_count(), 3);
    held.finish();
    scheduler.idle().await;
}

#[tokio::test(flavor = "current_thread")]
async fn topic_refresh_is_off_with_zero_interval_and_no_times() {
    // it("토픽 갱신을 끄면(간격 0·시각 없음) 하지 않는다")
    let t = Setup::new();
    write_num(
        &t.shared.db.lock().unwrap(),
        &TOPICS_INTERVAL_HOURS,
        0.0,
        start_time(),
    )
    .unwrap();
    let calls = Arc::new(Mutex::new(0));
    let counter = calls.clone();
    let _scheduler = t.start(Options {
        refresh_topics: Some(Arc::new(move || {
            *counter.lock().unwrap() += 1;
            Box::pin(async { true })
        })),
        ..Options::default()
    });
    assert_eq!(*calls.lock().unwrap(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn prices_every_minute_skipping_while_the_previous_round_is_running_and_off_with_jobs_off() {
    // it("주가(13단계): 1분마다 부르고, 앞 회차가 끝나지 않았으면 건너뛰며, 정기 작업을 끄면 부르지 않는다")
    let t = Setup::new();
    let finish: Arc<Mutex<Option<oneshot::Sender<()>>>> = Arc::new(Mutex::new(None));
    let calls = Arc::new(AtomicUsize::new(0));
    let refresh_prices: Arc<dyn Fn() -> BoxFuture<()> + Send + Sync> = {
        let (finish, calls) = (finish.clone(), calls.clone());
        Arc::new(move || {
            calls.fetch_add(1, Ordering::SeqCst);
            let (sender, receiver) = oneshot::channel();
            *finish.lock().unwrap() = Some(sender);
            Box::pin(async move {
                receiver.await.unwrap();
            })
        })
    };
    let scheduler = t.start(Options {
        refresh_prices: Some(refresh_prices),
        ..Options::default()
    });
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    t.advance(1);
    scheduler.tick();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    finish.lock().unwrap().take().unwrap().send(()).unwrap();
    scheduler.idle().await;
    t.advance(1);
    scheduler.tick();
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    let disabled_calls = Arc::new(AtomicUsize::new(0));
    let counter = disabled_calls.clone();
    let other = Setup::new();
    let _other_scheduler = other.start(Options {
        jobs_disabled: true,
        refresh_prices: Some(Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {})
        })),
        ..Options::default()
    });
    assert_eq!(disabled_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn a_panicking_job_still_clears_its_running_flag() {
    // JS의 `.finally`처럼 작업이 패닉으로 끝나도 "도는 중" 표시가 지워져 다음 회차가 돈다.
    let t = Setup::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let scheduler = t.start(Options {
        refresh_prices: Some(Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { panic!("주가 받기가 비정상 종료") })
        })),
        ..Options::default()
    });
    scheduler.idle().await;
    t.advance(1);
    scheduler.tick();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    scheduler.idle().await;
}

#[tokio::test(flavor = "current_thread")]
async fn the_timer_ticks_on_its_own_and_stop_ends_it() {
    // 타이머: 처음은 바로, 그 뒤 tick_ms마다. stop 뒤에는 더 부르지 않는다.
    let t = Setup::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let scheduler = start_scheduler(SchedulerDeps {
        db: t.shared.db.clone(),
        collector: t.collector.clone(),
        keyword_queue: t.keyword_queue.clone(),
        sync_listings: Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {})
        }),
        backup: None,
        refresh_topics: None,
        refresh_prices: None,
        jobs_enabled: false,
        now: Some({
            let clock = t.clock.clone();
            Arc::new(move || clock.fetch_add(31 * MIN, Ordering::SeqCst))
        }),
        tick_ms: Some(20),
    });
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    tokio::time::sleep(Duration::from_millis(70)).await;
    scheduler.stop();
    let after_stop = calls.load(Ordering::SeqCst);
    assert!(after_stop >= 2, "타이머가 부르지 않았다: {after_stop}");
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(calls.load(Ordering::SeqCst), after_stop);
}
