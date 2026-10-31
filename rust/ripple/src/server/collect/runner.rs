use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

use crate::server::ai::decider::types::Decider;
use crate::server::collect::finnhub::FinnhubNewsClient;
use crate::server::collect::http::HttpClient;
use crate::server::collect::robots::RobotsCheck;
use crate::server::collect::run::{CollectDeps, start_collection_run};
use crate::server::db::schema::RunTrigger;
use crate::server::log::log_error;

struct Inner<H, R, F, D> {
    deps: Arc<CollectDeps<H, R, F, D>>,
    /// 지금 도는 회차 id
    current: Mutex<Option<i64>>,
    notify: Notify,
}

/// 수집 회차를 하나씩만 돌린다(같은 수집이 두 번 돌지 않게, SPEC FR-6). tokio 런타임 안에서만 쓴다.
pub struct CollectionRunner<H, R, F, D> {
    inner: Arc<Inner<H, R, F, D>>,
}

impl<H, R, F, D> Clone for CollectionRunner<H, R, F, D> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<H, R, F, D> CollectionRunner<H, R, F, D>
where
    H: HttpClient + 'static,
    R: RobotsCheck + 'static,
    F: FinnhubNewsClient + 'static,
    D: Decider + 'static,
{
    pub fn new(deps: Arc<CollectDeps<H, R, F, D>>) -> Self {
        Self {
            inner: Arc::new(Inner {
                deps,
                current: Mutex::new(None),
                notify: Notify::new(),
            }),
        }
    }

    /// 수집을 시작하고 회차 id를 돌려준다. 이미 도는 중이면 None(같은 수집이 두 번 돌지 않게, SPEC FR-6).
    pub fn start(&self, trigger: RunTrigger) -> Option<i64> {
        let mut current = self
            .inner
            .current
            .lock()
            .expect("수집 실행기 잠금이 깨졌습니다");
        if current.is_some() {
            return None;
        }
        let (run_id, done) = match start_collection_run(&self.inner.deps, trigger) {
            Ok(started) => started,
            Err(error) => {
                log_error("수집 회차를 시작하지 못했습니다", &error);
                return None;
            }
        };
        *current = Some(run_id);
        drop(current);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            // 회차가 어떻게 끝나든(작업이 비정상 종료해도) 도는 중 표시를 지운다.
            let _ = done.await;
            *inner.current.lock().expect("수집 실행기 잠금이 깨졌습니다") = None;
            inner.notify.notify_waiters();
        });
        Some(run_id)
    }

    /// 지금 도는 회차 id
    pub fn running_id(&self) -> Option<i64> {
        *self
            .inner
            .current
            .lock()
            .expect("수집 실행기 잠금이 깨졌습니다")
    }

    /// 지금 도는 회차가 끝날 때까지 기다린다(테스트·종료용).
    pub async fn idle(&self) {
        loop {
            let notified = self.inner.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.running_id().is_none() {
                return;
            }
            notified.await;
        }
    }
}
