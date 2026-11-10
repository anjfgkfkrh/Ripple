use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use super::*;
use crate::server::ai::llm::{JsonResult, LlmError, LlmErrorKind, LlmUsage, StructuredRequest};
use crate::server::db::schema::{Market, UsageProvider};
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

fn queue_of<L: LlmClient + 'static>(shared: &SharedTestDb, llm: Arc<L>) -> KeywordQueue<L> {
    KeywordQueue::new(shared.db.clone(), llm, None)
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
        depth: 2,
        deepest: 1,
        dropped: 2,
        stat_words: 12,
        aliases: 3,
        anchors: 11,
        update: None,
    };
    let line = success_line(7, &ok, "1.2");
    assert_eq!(
        line,
        "키워드 생성: 종목 id 7, 6개(깊이 1/2, 규칙에 안 맞아 버림 2개, 별칭 3개·고유어 11개, 통계 낱말 12개 참고), 1.2초"
    );
    let market = GenerateOk {
        count: 4,
        depth: 2,
        deepest: 2,
        dropped: 0,
        stat_words: 0,
        aliases: 0,
        anchors: 0,
        update: None,
    };
    assert_eq!(
        success_line(8, &market, "0.5"),
        "키워드 생성: 종목 id 8, 4개(깊이 2/2), 0.5초"
    );
}

/// 전체 생성 요청에는 표본 트리를, 정기 점검 요청에는 빈 답을 주는 가짜 LLM
fn dual_llm() -> Arc<SlowLlm> {
    Arc::new(SlowLlm {
        inner: fake_llm(
            |messages| {
                if messages[0].content.contains("주 1회 점검") {
                    serde_json::json!({
                        "remove": [], "add": [],
                        "anchorsRemove": [], "anchorsAdd": [], "aliasesAdd": []
                    })
                } else {
                    sample_keyword_tree(false)
                }
            },
            USAGE,
        ),
        active: AtomicUsize::new(0),
        max_active: AtomicUsize::new(0),
    })
}

fn schemas(llm: &SlowLlm) -> Vec<String> {
    llm.inner
        .calls
        .lock()
        .unwrap()
        .iter()
        .map(|c| c.schema_name.clone())
        .collect()
}

#[tokio::test(flavor = "current_thread")]
async fn periodic_is_incremental_with_a_tree_and_full_without_one() {
    let shared = seeded();
    let ids = register_three(&shared);
    let llm = dual_llm();
    let queue = queue_of(&shared, llm.clone());

    // 등록·"다시 만들기"는 전체 생성
    queue.enqueue(ids[0]);
    queue.idle().await;
    queue.enqueue(ids[0]);
    queue.idle().await;
    // 정기 점검: 트리가 있으면 증분, 없으면 전체 생성
    queue.enqueue_periodic(ids[0]);
    queue.enqueue_periodic(ids[1]);
    queue.idle().await;

    assert_eq!(
        schemas(&llm),
        [
            "keyword_tree",
            "keyword_tree",
            "keyword_update",
            "keyword_tree"
        ]
    );
    for id in &ids[..2] {
        assert_eq!(status(&shared, *id), "ready");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn periodic_falls_back_to_full_when_the_stock_has_no_anchors() {
    let shared = seeded();
    let ids = register_three(&shared);
    let llm = dual_llm();
    let queue = queue_of(&shared, llm.clone());
    queue.enqueue(ids[0]);
    queue.idle().await;
    shared
        .db
        .lock()
        .unwrap()
        .execute("UPDATE stocks SET anchors = '[]' WHERE id = ?1", [ids[0]])
        .unwrap();

    queue.enqueue_periodic(ids[0]);
    queue.idle().await;

    assert_eq!(schemas(&llm), ["keyword_tree", "keyword_tree"]);
}

#[tokio::test(flavor = "current_thread")]
async fn a_full_request_overrides_a_waiting_periodic_one_but_not_the_reverse() {
    let shared = seeded();
    let ids = register_three(&shared);
    let llm = dual_llm();
    let queue = queue_of(&shared, llm.clone());
    // 트리를 둘에게 만들어 둔다.
    queue.enqueue(ids[0]);
    queue.enqueue(ids[1]);
    queue.idle().await;

    // 기다리는 정기 점검 위에 전체 생성이 덮는다.
    queue.enqueue_periodic(ids[0]); // 만드는 중
    queue.enqueue_periodic(ids[1]); // 기다림(정기)
    queue.enqueue(ids[1]); // 전체 생성으로 바뀐다
    queue.idle().await;
    assert_eq!(schemas(&llm)[2..], ["keyword_update", "keyword_tree"]);

    // 기다리는 전체 생성은 정기 점검이 덮지 못한다.
    queue.enqueue(ids[0]); // 만드는 중
    queue.enqueue(ids[1]); // 기다림(전체)
    queue.enqueue_periodic(ids[1]); // 무시
    queue.idle().await;
    assert_eq!(schemas(&llm)[4..], ["keyword_tree", "keyword_tree"]);
}

#[test]
fn update_line_shows_what_changed() {
    use crate::server::keywords::generate::GenerateOk;
    use crate::server::keywords::update::UpdateSummary;
    let update = UpdateSummary {
        removed: 1,
        protected: 2,
        added: 3,
        evicted: 4,
        anchors_removed: 1,
        anchors_added: 2,
        aliases_added: 1,
        count: 14,
        deepest: 2,
        anchors: 9,
        aliases: 2,
        dropped: 1,
    };
    let ok = GenerateOk {
        count: 14,
        depth: 2,
        deepest: 2,
        dropped: 1,
        stat_words: 5,
        aliases: 2,
        anchors: 9,
        update: Some(Box::new(update)),
    };
    assert_eq!(
        success_line(7, &ok, "1.2"),
        "키워드 점검: 종목 id 7, 지금 14개(뺌 1, 더함 3, 상한으로 밀어냄 4, 실적으로 지킴 2, 고유어 -1+2, 별칭 +1, 규칙에 안 맞아 버림 1), 1.2초"
    );
}
