use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use super::*;
use crate::server::ai::llm::{JsonResult, LlmError, LlmErrorKind, LlmUsage, StructuredRequest};
use crate::server::db::schema::{Market, UsageProvider};
use crate::server::keywords::generate::NoDecider;
use crate::server::stocks::repository::{RegisterInput, RegisterResult, register_stock};
use crate::test_support::{
    FakeLlm, SharedTestDb, fake_llm, sample_keyword_tree, seed_listings, shared_test_db,
};

const USAGE: LlmUsage = LlmUsage {
    input_tokens: 500,
    output_tokens: 60,
    cost_usd: 0.0001,
};

fn seeded() -> SharedTestDb {
    let shared = shared_test_db();
    seed_listings(&mut shared.db.lock().unwrap());
    shared
}

/// 표본의 한국 종목 3개를 등록한다.
fn register_three(shared: &SharedTestDb) -> Vec<i64> {
    let conn = shared.db.lock().unwrap();
    ["005930", "000660", "000720"]
        .iter()
        .map(|code| {
            match register_stock(
                &conn,
                &RegisterInput {
                    market: Market::Kr,
                    code,
                    user_input: code,
                    name_ko: None,
                },
                crate::server::time::now_ms(),
            )
            .unwrap()
            {
                RegisterResult::Created(stock) => stock.id,
                _ => panic!("{code}"),
            }
        })
        .collect()
}

fn status(shared: &SharedTestDb, id: i64) -> String {
    shared
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT keywords_status FROM stocks WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap()
}

/// 부를 때마다 동시에 몇 개가 돌고 있는지 재는 가짜 LLM
struct SlowLlm {
    inner: FakeLlm,
    active: AtomicUsize,
    max_active: AtomicUsize,
}

fn slow_llm() -> Arc<SlowLlm> {
    Arc::new(SlowLlm {
        inner: fake_llm(|_| sample_keyword_tree(false), USAGE),
        active: AtomicUsize::new(0),
        max_active: AtomicUsize::new(0),
    })
}

impl LlmClient for SlowLlm {
    fn provider(&self) -> UsageProvider {
        UsageProvider::Openrouter
    }

    async fn generate_json<T: Send>(
        &self,
        request: &StructuredRequest<T>,
    ) -> Result<JsonResult, LlmError> {
        let now_active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(now_active, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(10)).await;
        let result = self.inner.generate_json(request).await;
        self.active.fetch_sub(1, Ordering::SeqCst);
        result
    }
}

fn queue_of<L: LlmClient + 'static>(
    shared: &SharedTestDb,
    llm: Arc<L>,
) -> KeywordQueue<L, NoDecider> {
    KeywordQueue::new(shared.db.clone(), llm, None, None)
}

// describe("키워드 생성 대기열 (D-37)")
#[tokio::test(flavor = "current_thread")]
async fn enqueue_marks_pending_at_once_and_builds_one_by_one() {
    // it("넣으면 바로 대기 상태가 되고, 하나씩 차례로 만든다")
    let shared = seeded();
    let ids = register_three(&shared);
    shared
        .db
        .lock()
        .unwrap()
        .execute("UPDATE stocks SET keywords_status = 'ready'", [])
        .unwrap();
    let slow = slow_llm();
    let queue = queue_of(&shared, slow.clone());

    for id in &ids {
        queue.enqueue(*id);
    }
    // 첫 종목은 바로 만들기 시작하고, 나머지는 기다린다.
    let statuses: Vec<String> = ids.iter().map(|id| status(&shared, *id)).collect();
    assert_eq!(statuses, ["generating", "pending", "pending"]);

    queue.idle().await;
    let statuses: Vec<String> = ids.iter().map(|id| status(&shared, *id)).collect();
    assert_eq!(statuses, ["ready", "ready", "ready"]);
    assert_eq!(slow.inner.call_count(), 3);
    assert_eq!(slow.max_active.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn enqueueing_a_waiting_or_running_stock_again_is_ignored() {
    // it("이미 기다리거나 만드는 중인 종목을 다시 넣으면 무시한다")
    let shared = seeded();
    let ids = register_three(&shared);
    let (first, second) = (ids[0], ids[1]);
    let slow = slow_llm();
    let queue = queue_of(&shared, slow.clone());

    queue.enqueue(first);
    queue.enqueue(second);
    queue.enqueue(first); // 만드는 중
    queue.enqueue(second); // 기다리는 중
    queue.idle().await;
    assert_eq!(slow.inner.call_count(), 2);

    // 끝난 뒤에 넣으면 다시 만든다.
    queue.enqueue(first);
    queue.idle().await;
    assert_eq!(slow.inner.call_count(), 3);
}

/// 첫 호출은 실패하고 이후는 표본 트리를 주는 가짜
struct FailsFirst {
    calls: AtomicUsize,
    inner: FakeLlm,
}

impl LlmClient for FailsFirst {
    fn provider(&self) -> UsageProvider {
        UsageProvider::Openrouter
    }

    async fn generate_json<T: Send>(
        &self,
        request: &StructuredRequest<T>,
    ) -> Result<JsonResult, LlmError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            return Err(LlmError::new("x", LlmErrorKind::Upstream));
        }
        self.inner.generate_json(request).await
    }
}

#[tokio::test(flavor = "current_thread")]
async fn one_failing_stock_does_not_stop_the_next() {
    // it("한 종목이 실패해도 다음 종목은 계속 만든다")
    let shared = seeded();
    let ids = register_three(&shared);
    let llm = Arc::new(FailsFirst {
        calls: AtomicUsize::new(0),
        inner: fake_llm(|_| sample_keyword_tree(false), USAGE),
    });
    let queue = queue_of(&shared, llm);

    queue.enqueue(ids[0]);
    queue.enqueue(ids[1]);
    queue.idle().await;
    assert_eq!(
        [status(&shared, ids[0]), status(&shared, ids[1])],
        ["failed", "ready"]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn idle_returns_at_once_when_empty() {
    // it("비어 있으면 idle은 바로 끝난다")
    let shared = seeded();
    let queue = queue_of(&shared, slow_llm());
    tokio::time::timeout(Duration::from_millis(500), queue.idle())
        .await
        .expect("idle이 끝나지 않았다");
}

#[tokio::test(flavor = "current_thread")]
async fn deleted_stock_in_queue_is_skipped_with_a_failure_log_and_the_queue_continues() {
    // Rust에서만 생기는 경우: 만들 수 없는 종목(삭제됨)이 끼어도 대기열은 다음 종목으로 간다.
    let shared = seeded();
    let ids = register_three(&shared);
    shared
        .db
        .lock()
        .unwrap()
        .execute("UPDATE stocks SET deleted_at = 1 WHERE id = ?1", [ids[0]])
        .unwrap();
    let slow = slow_llm();
    let queue = queue_of(&shared, slow.clone());
    queue.enqueue(ids[0]);
    queue.enqueue(ids[1]);
    queue.idle().await;
    assert_eq!(slow.inner.call_count(), 1);
    assert_eq!(status(&shared, ids[1]), "ready");
}

#[tokio::test(flavor = "current_thread")]
async fn queue_is_cloneable_and_shares_state() {
    let shared = seeded();
    let ids = register_three(&shared);
    let slow = slow_llm();
    let queue = queue_of(&shared, slow.clone());
    let other = queue.clone();
    queue.enqueue(ids[0]);
    other.enqueue(ids[0]); // 같은 대기열이라 무시된다
    other.idle().await;
    assert_eq!(slow.inner.call_count(), 1);
}

#[test]
fn success_line_shows_actual_over_configured_depth() {
    use crate::server::keywords::generate::GenerateOk;
    let ok = GenerateOk {
        count: 6,
        depth: 3,
        deepest: 1,
        dropped: 0,
        candidates: 8,
        rated: 8,
        unrated_reason: None,
        headlines: 0,
    };
    let line = success_line(7, &ok, "1.2");
    assert!(line.contains("(깊이 1/3, Jev 평가 8개)"), "{line}");
}
