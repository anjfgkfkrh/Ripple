use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::Notify;

use crate::server::ai::llm::LlmClient;
use crate::server::db::client::Db;
use crate::server::db::schema::KeywordStatus;
use crate::server::keywords::generate::{
    GenerateMode, GenerateOptions, GenerateResult, PreparedGeneration, prepare_generation_with,
    run_generation, set_keyword_status,
};
use crate::server::keywords::update::UpdateSummary;
use crate::server::log::{log, log_error};
use crate::server::text::js_to_fixed;
use crate::server::time::EpochMs;

type NowFn = Arc<dyn Fn() -> EpochMs + Send + Sync>;

#[derive(Default)]
struct State {
    waiting: VecDeque<i64>,
    queued: HashSet<i64>,
    /// 기다리는 항목마다 만드는 방식(전체 생성·정기 점검)
    modes: HashMap<i64, GenerateMode>,
    current: Option<i64>,
    running: bool,
}

struct Inner<L> {
    db: Db,
    llm: Arc<L>,
    now: Option<NowFn>,
    state: Mutex<State>,
    notify: Notify,
}

/// worker 안의 키워드 생성 대기열(D-37). 한 번에 하나씩 만들어 LLM 호출이 몰리지 않게 한다.
/// 등록 응답은 기다리지 않고 바로 돌아간다. tokio 런타임 안에서만 쓴다.
pub struct KeywordQueue<L> {
    inner: Arc<Inner<L>>,
}

impl<L> Clone for KeywordQueue<L> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<L> KeywordQueue<L>
where
    L: LlmClient + 'static,
{
    pub fn new(db: Db, llm: Arc<L>, now: Option<NowFn>) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                llm,
                now,
                state: Mutex::new(State::default()),
                notify: Notify::new(),
            }),
        }
    }

    /// 전체 생성 대기열에 넣는다(등록·"다시 만들기"·대기 중 이어 만들기). 이미 기다리거나 만드는 중인 종목이면 무시한다.
    /// 기다리는 종목이 정기 점검이었으면 전체 생성으로 바꾼다. 오류를 밖으로 내지 않는다.
    pub fn enqueue(&self, stock_id: i64) {
        self.enqueue_with(stock_id, GenerateMode::Full);
    }

    /// 정기 점검으로 대기열에 넣는다. 실행할 때 트리가 없거나 종목의 고유어가 비면 전체 생성으로 바뀐다.
    pub fn enqueue_periodic(&self, stock_id: i64) {
        self.enqueue_with(stock_id, GenerateMode::Periodic);
    }

    fn enqueue_with(&self, stock_id: i64, mode: GenerateMode) {
        let inner = &self.inner;
        {
            let mut state = inner.state.lock().expect("대기열 잠금이 깨졌습니다");
            if state.queued.contains(&stock_id) {
                // 전체 생성이 정기 점검을 덮는다(반대는 아니다).
                if mode == GenerateMode::Full {
                    state.modes.insert(stock_id, GenerateMode::Full);
                }
                return;
            }
            if state.current == Some(stock_id) {
                return;
            }
        }
        {
            let conn = inner.db.lock().expect("DB 잠금이 깨졌습니다");
            if let Err(error) = set_keyword_status(&conn, stock_id, KeywordStatus::Pending, None) {
                // 상태 표시만 늦을 뿐, 생성은 그대로 진행한다.
                log_error(
                    &format!("키워드 대기 상태 기록 실패: 종목 id {stock_id}"),
                    &error,
                );
            }
        }
        let start_now = {
            let mut state = inner.state.lock().expect("대기열 잠금이 깨졌습니다");
            state.queued.insert(stock_id);
            state.modes.insert(stock_id, mode);
            state.waiting.push_back(stock_id);
            if state.running {
                false
            } else {
                state.running = true;
                true
            }
        };
        if start_now {
            // TS는 async 함수가 첫 await까지 바로 실행되므로, 첫 종목은 넣자마자 '만드는 중'이 된다.
            let first = self
                .take_next()
                .map(|(id, mode)| (id, prepare_generation_with(&inner.db, id, self.now(), mode)));
            let queue = self.clone();
            tokio::spawn(async move { queue.drain(first).await });
        }
    }

    /// 대기열이 빌 때까지 기다린다(테스트·종료용).
    pub async fn idle(&self) {
        loop {
            let notified = self.inner.notify.notified();
            tokio::pin!(notified);
            // 확인하기 전에 등록해 두어야 그 사이에 끝나도 놓치지 않는다.
            notified.as_mut().enable();
            if !self
                .inner
                .state
                .lock()
                .expect("대기열 잠금이 깨졌습니다")
                .running
            {
                return;
            }
            notified.await;
        }
    }

    fn now(&self) -> EpochMs {
        self.inner
            .now
            .as_ref()
            .map_or_else(crate::server::time::now_ms, |f| f())
    }

    /// 다음 종목을 꺼내 '만드는 중'으로 둔다. 없으면 대기열을 끝내고 None
    fn take_next(&self) -> Option<(i64, GenerateMode)> {
        let mut state = self.inner.state.lock().expect("대기열 잠금이 깨졌습니다");
        match state.waiting.pop_front() {
            Some(id) => {
                state.queued.remove(&id);
                let mode = state.modes.remove(&id).unwrap_or(GenerateMode::Full);
                state.current = Some(id);
                Some((id, mode))
            }
            None => {
                state.running = false;
                state.current = None;
                drop(state);
                self.inner.notify.notify_waiters();
                None
            }
        }
    }

    async fn drain(&self, mut first: Option<(i64, Result<PreparedGeneration, GenerateResult>)>) {
        loop {
            let (stock_id, prepared) = match first.take() {
                Some(item) => item,
                None => match self.take_next() {
                    Some((id, mode)) => (
                        id,
                        prepare_generation_with(&self.inner.db, id, self.now(), mode),
                    ),
                    None => return,
                },
            };
            let started = Instant::now();
            let options = GenerateOptions {
                now: self
                    .inner
                    .now
                    .as_deref()
                    .map(|f| f as &(dyn Fn() -> EpochMs + Send + Sync)),
            };
            let result = match prepared {
                Ok(prepared) => {
                    run_generation(&self.inner.db, &*self.inner.llm, prepared, &options).await
                }
                Err(result) => result,
            };
            let seconds = js_to_fixed(started.elapsed().as_secs_f64(), 1);
            match result {
                GenerateResult::Ok(ok) => {
                    log(&success_line(stock_id, &ok, &seconds));
                }
                GenerateResult::Err(error) => {
                    log(&format!(
                        "키워드 생성 실패: 종목 id {stock_id}, {error}, {seconds}초"
                    ));
                }
            }
            self.inner
                .state
                .lock()
                .expect("대기열 잠금이 깨졌습니다")
                .current = None;
        }
    }
}

/// 성공 결과 한 줄. 깊이는 `실제/설정`으로 보인다(실제는 만든 트리의 가장 깊은 단계).
fn success_line(
    stock_id: i64,
    ok: &crate::server::keywords::generate::GenerateOk,
    seconds: &str,
) -> String {
    if let Some(update) = &ok.update {
        return update_line(stock_id, update, seconds);
    }
    let dropped = if ok.dropped > 0 {
        format!(", 규칙에 안 맞아 버림 {}개", ok.dropped)
    } else {
        String::new()
    };
    let stat = if ok.stat_words > 0 {
        format!(", 통계 낱말 {}개 참고", ok.stat_words)
    } else {
        String::new()
    };
    let terms = if ok.anchors > 0 {
        format!(", 별칭 {}개·고유어 {}개", ok.aliases, ok.anchors)
    } else {
        String::new()
    };
    format!(
        "키워드 생성: 종목 id {stock_id}, {}개(깊이 {}/{}{dropped}{terms}{stat}), {seconds}초",
        ok.count, ok.deepest, ok.depth
    )
}

/// 정기 점검 결과 한 줄
fn update_line(stock_id: i64, update: &UpdateSummary, seconds: &str) -> String {
    let mut parts = vec![
        format!("뺌 {}", update.removed),
        format!("더함 {}", update.added),
        format!("상한으로 밀어냄 {}", update.evicted),
        format!("실적으로 지킴 {}", update.protected),
    ];
    if update.anchors > 0 {
        parts.push(format!(
            "고유어 -{}+{}",
            update.anchors_removed, update.anchors_added
        ));
        parts.push(format!("별칭 +{}", update.aliases_added));
    }
    if update.dropped > 0 {
        parts.push(format!("규칙에 안 맞아 버림 {}", update.dropped));
    }
    format!(
        "키워드 점검: 종목 id {stock_id}, 지금 {}개({}), {seconds}초",
        update.count,
        parts.join(", ")
    )
}

#[cfg(test)]
#[path = "queue_tests.rs"]
mod tests;
