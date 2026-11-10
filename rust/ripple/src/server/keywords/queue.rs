use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::Notify;

use crate::server::ai::decider::types::Decider;
use crate::server::ai::llm::LlmClient;
use crate::server::db::client::Db;
use crate::server::db::schema::KeywordStatus;
use crate::server::keywords::generate::{
    GenerateOptions, GenerateResult, PreparedGeneration, prepare_generation, run_generation,
    set_keyword_status,
};
use crate::server::log::{log, log_error};
use crate::server::text::js_to_fixed;
use crate::server::time::EpochMs;

type NowFn = Arc<dyn Fn() -> EpochMs + Send + Sync>;

#[derive(Default)]
struct State {
    waiting: VecDeque<i64>,
    queued: HashSet<i64>,
    current: Option<i64>,
    running: bool,
}

struct Inner<L, D> {
    db: Db,
    llm: Arc<L>,
    /// 후보 키워드를 관련도·영향도로 고르는 Jev(11단계). 키가 없으면 None(자르지 않음)
    decider: Option<Arc<D>>,
    now: Option<NowFn>,
    state: Mutex<State>,
    notify: Notify,
}

/// worker 안의 키워드 생성 대기열(D-37). 한 번에 하나씩 만들어 LLM 호출이 몰리지 않게 한다.
/// 등록 응답은 기다리지 않고 바로 돌아간다. tokio 런타임 안에서만 쓴다.
pub struct KeywordQueue<L, D> {
    inner: Arc<Inner<L, D>>,
}

impl<L, D> Clone for KeywordQueue<L, D> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<L, D> KeywordQueue<L, D>
where
    L: LlmClient + 'static,
    D: Decider + 'static,
{
    pub fn new(db: Db, llm: Arc<L>, decider: Option<Arc<D>>, now: Option<NowFn>) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                llm,
                decider,
                now,
                state: Mutex::new(State::default()),
                notify: Notify::new(),
            }),
        }
    }

    /// 생성 대기열에 넣는다. 이미 기다리거나 만드는 중인 종목이면 무시한다. 오류를 밖으로 내지 않는다.
    pub fn enqueue(&self, stock_id: i64) {
        let inner = &self.inner;
        {
            let state = inner.state.lock().expect("대기열 잠금이 깨졌습니다");
            if state.queued.contains(&stock_id) || state.current == Some(stock_id) {
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
                .map(|id| (id, prepare_generation(&inner.db, id, self.now())));
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
    fn take_next(&self) -> Option<i64> {
        let mut state = self.inner.state.lock().expect("대기열 잠금이 깨졌습니다");
        match state.waiting.pop_front() {
            Some(id) => {
                state.queued.remove(&id);
                state.current = Some(id);
                Some(id)
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
                    Some(id) => (id, prepare_generation(&self.inner.db, id, self.now())),
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
                decider: self.inner.decider.as_deref(),
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
    let dropped = if ok.dropped > 0 {
        format!(", 규칙에 안 맞아 버림 {}개", ok.dropped)
    } else {
        String::new()
    };
    let headlines = if ok.headlines > 0 {
        format!(", 기사 제목 {}개 참고", ok.headlines)
    } else {
        String::new()
    };
    let rated = match &ok.unrated_reason {
        Some(reason) => format!(", 평가 못 해 자르지 않음: {reason}"),
        None => format!(", Jev 평가 {}개", ok.rated),
    };
    format!(
        "키워드 생성: 종목 id {stock_id}, 후보 {}개 → {}개(깊이 {}/{}{dropped}{rated}{headlines}), {seconds}초",
        ok.candidates, ok.count, ok.deepest, ok.depth
    )
}

#[cfg(test)]
#[path = "queue_tests.rs"]
mod tests;
