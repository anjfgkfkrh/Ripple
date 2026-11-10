use std::sync::Mutex;

use serde_json::{Value, json};

use super::*;
use crate::server::ai::llm::{JsonResult, LlmError, LlmMessage, LlmUsage, StructuredRequest};
use crate::server::budget::usage::{UsageEntry, record_usage};
use crate::server::db::schema::{KeywordKind, UsageProvider};
use crate::server::keywords::generate::replace_keywords;
use crate::server::keywords::tree::KeywordNode;
use crate::server::settings::{TOPICS_REFRESHED_AT, read_num};
use crate::server::settings_form::read_settings_form;
use crate::server::stocks::repository::list_active_stocks;
use crate::server::time::now_ms;
use crate::server::topics::read::{list_market_items, list_user_topic_items};
use crate::server::topics::store::ensure_default_market;
use crate::test_support::{
    FakeLlm, LinkSpec, SharedTestDb, fake_llm, insert_link, ms, seed_listings, shared_test_db,
};

const TOKEN: &str = "worker-test-token-0123456789abcdef";

const USAGE: LlmUsage = LlmUsage {
    input_tokens: 500,
    output_tokens: 60,
    cost_usd: 0.0001,
};

/// 넣은 종목 id만 남기는 가짜 키워드 대기열(`fakeKeywordQueue`)
#[derive(Default)]
struct FakeQueue {
    enqueued: Mutex<Vec<i64>>,
}

impl KeywordEnqueuer for FakeQueue {
    fn enqueue(&self, stock_id: i64) {
        self.enqueued.lock().unwrap().push(stock_id);
    }
}

impl FakeQueue {
    fn list(&self) -> Vec<i64> {
        self.enqueued.lock().unwrap().clone()
    }

    fn clear(&self) {
        self.enqueued.lock().unwrap().clear();
    }
}

/// 시작한 종류를 남기는 가짜 수집기(`fakeCollector`)
#[derive(Default)]
struct FakeCollector {
    started: Mutex<Vec<RunTrigger>>,
    running: Mutex<Option<i64>>,
}

impl Collector for FakeCollector {
    fn start(&self, trigger: RunTrigger) -> Option<i64> {
        let mut running = self.running.lock().unwrap();
        if running.is_some() {
            return None;
        }
        let mut started = self.started.lock().unwrap();
        started.push(trigger);
        *running = Some(started.len() as i64);
        *running
    }

    fn running_id(&self) -> Option<i64> {
        *self.running.lock().unwrap()
    }
}

impl FakeCollector {
    fn finish(&self) {
        *self.running.lock().unwrap() = None;
    }

    fn started(&self) -> Vec<&'static str> {
        self.started
            .lock()
            .unwrap()
            .iter()
            .map(|t| t.as_str())
            .collect()
    }
}

#[derive(Default)]
struct FakeRefresher {
    running: Mutex<bool>,
    calls: Mutex<usize>,
}

impl TopicRefreshControl for FakeRefresher {
    fn running(&self) -> bool {
        *self.running.lock().unwrap()
    }

    fn start_refresh(&self) {
        *self.calls.lock().unwrap() += 1;
        *self.running.lock().unwrap() = true;
    }
}

/// 항상 실패하는 LLM
struct FailingLlm(LlmErrorKind);

impl LlmClient for FailingLlm {
    fn provider(&self) -> UsageProvider {
        UsageProvider::Openrouter
    }

    async fn generate_json<T: Send>(
        &self,
        _: &StructuredRequest<T>,
    ) -> Result<JsonResult, LlmError> {
        Err(LlmError::new("x", self.0))
    }
}

fn stock_llm() -> FakeLlm {
    fake_llm(
        |messages: &[LlmMessage]| {
            let query = messages.last().map_or("", |m| m.content.as_str());
            match query {
                "삼전" => json!({"candidates": [
                    {"market": "KR", "code": "005930", "name": "삼성전자", "nameKo": "삼성전자"}
                ]}),
                "NVDA" | "엔비디아" => json!({"candidates": [
                    {"market": "US", "code": "NVDA", "name": "NVIDIA Corporation", "nameKo": "엔비디아"}
                ]}),
                _ => json!({"candidates": []}),
            }
        },
        USAGE,
    )
}

struct Started {
    base_url: String,
    shared: SharedTestDb,
    queue: Arc<FakeQueue>,
    collector: Arc<FakeCollector>,
    http: reqwest::Client,
}

struct Options {
    seeded: bool,
    check_model: CheckModelFn,
    refresher: Option<Arc<dyn TopicRefreshControl>>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            seeded: true,
            check_model: always(true),
            refresher: None,
        }
    }
}

fn always(known: bool) -> CheckModelFn {
    Arc::new(move |_| Box::pin(async move { Ok(known) }))
}

async fn start() -> Started {
    start_with(stock_llm(), Options::default()).await
}

async fn start_with<L: LlmClient + 'static>(llm: L, options: Options) -> Started {
    let shared = shared_test_db();
    if options.seeded {
        seed_listings(&mut shared.db.lock().unwrap());
    }
    let queue = Arc::new(FakeQueue::default());
    let collector = Arc::new(FakeCollector::default());
    let app = worker_router(WorkerDeps {
        db: shared.db.clone(),
        api_token: TOKEN.to_string(),
        llm: Arc::new(llm),
        keyword_queue: queue.clone(),
        collector: collector.clone(),
        check_model: options.check_model,
        topic_refresher: options.refresher,
        now: None,
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Started {
        base_url,
        shared,
        queue,
        collector,
        http: reqwest::Client::new(),
    }
}

impl Started {
    /// `token`을 `None`으로 주면 인증 헤더를 보내지 않는다.
    async fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<Body_>,
        token: Option<&str>,
    ) -> (u16, Value) {
        let mut request = self
            .http
            .request(method.parse().unwrap(), format!("{}{path}", self.base_url))
            .header("Content-Type", "application/json");
        if let Some(token) = token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        request = match body {
            Some(Body_::Json(value)) => request.body(value.to_string()),
            Some(Body_::Text(text)) => request.body(text),
            None => request,
        };
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    async fn post(&self, path: &str, body: Value) -> (u16, Value) {
        self.call("POST", path, Some(Body_::Json(body)), Some(TOKEN))
            .await
    }

    async fn put(&self, path: &str, body: Value) -> (u16, Value) {
        self.call("PUT", path, Some(Body_::Json(body)), Some(TOKEN))
            .await
    }

    async fn plain(&self, method: &str, path: &str) -> (u16, Value) {
        self.call(method, path, None, Some(TOKEN)).await
    }
}

enum Body_ {
    Json(Value),
    Text(String),
}

// ---- describe("worker 내부 API: 공통") ----

#[tokio::test]
async fn health_returns_db_state_and_listing_counts_without_token() {
    // it("GET /health는 토큰 없이 DB 상태와 공식 목록 건수를 준다")
    let server = start().await;
    let (status, body) = server.call("GET", "/health", None, None).await;
    assert_eq!(status, 200);
    assert_eq!(
        body,
        json!({"status": "ok", "db": "ok", "listings": {"KR": 7, "US": 9}})
    );
}

#[tokio::test]
async fn health_is_503_when_db_connection_is_broken() {
    // it("GET /health: DB 연결이 끊기면 503") — 연결 잠금을 깨뜨려(패닉) 끊긴 연결을 흉내 낸다.
    let server = start().await;
    let db = server.shared.db.clone();
    let _ = std::thread::spawn(move || {
        let _guard = db.lock().unwrap();
        panic!("연결을 끊는다");
    })
    .join();
    let (status, body) = server.call("GET", "/health", None, None).await;
    assert_eq!(status, 503);
    assert_eq!(body, json!({"status": "error", "db": "error"}));
}

#[tokio::test]
async fn other_paths_need_a_valid_token() {
    // it("다른 경로는 토큰이 없거나 틀리면 401")
    let server = start().await;
    let body = || Some(Body_::Json(json!({"query": "삼전"})));
    let (none, _) = server.call("POST", "/stocks/match", body(), None).await;
    assert_eq!(none, 401);
    let wrong = format!("{TOKEN}x");
    let (bad, payload) = server
        .call("POST", "/stocks/match", body(), Some(&wrong))
        .await;
    assert_eq!(bad, 401);
    assert_eq!(payload, json!({"error": "unauthorized"}));
}

#[tokio::test]
async fn unknown_path_404_bad_body_400_big_body_413() {
    // it("없는 경로는 404, 잘못된 본문은 400, 너무 큰 본문은 413")
    let server = start().await;
    assert_eq!(server.post("/nope", json!({})).await.0, 404);
    let text = |t: &str| Some(Body_::Text(t.to_string()));
    assert_eq!(
        server
            .call("POST", "/stocks/match", text("{not json"), Some(TOKEN))
            .await
            .0,
        400
    );
    assert_eq!(
        server.post("/stocks/match", json!({"query": ""})).await.0,
        400
    );
    assert_eq!(
        server
            .post("/stocks/match", json!({"query": "x".repeat(51)}))
            .await
            .0,
        400
    );
    let (status, body) = server
        .post("/stocks/match", json!({"query": "x".repeat(20_000)}))
        .await;
    assert_eq!(status, 413);
    assert_eq!(body, json!({"error": "body_too_large"}));
}

// ---- describe("worker 내부 API: 종목 찾기") ----

#[tokio::test]
async fn match_returns_candidates_checked_against_official_list() {
    // it("POST /stocks/match → 공식 목록과 대조한 후보")
    let server = start().await;
    let (status, body) = server.post("/stocks/match", json!({"query": "삼전"})).await;
    assert_eq!(status, 200);
    assert_eq!(
        body,
        json!({"candidates": [{
            "market": "KR",
            "code": "005930",
            "name": "삼성전자",
            "nameKo": "삼성전자",
            "exchange": "KOSPI",
            "isEtf": false,
            "alreadyRegistered": false,
        }]})
    );
}

#[tokio::test]
async fn match_is_402_with_spend_and_limit_when_budget_is_used_up() {
    // it("예산을 다 썼으면 402와 사용액·한도")
    let server = start().await;
    record_usage(
        &server.shared.db.lock().unwrap(),
        &UsageEntry {
            provider: UsageProvider::Openrouter,
            model: "m",
            purpose: "t",
            usage: LlmUsage {
                input_tokens: 1,
                output_tokens: 1,
                cost_usd: 5.0,
            },
            at: now_ms(),
            stock_id: None,
        },
    )
    .unwrap();
    let (status, body) = server.post("/stocks/match", json!({"query": "삼전"})).await;
    assert_eq!(status, 402);
    assert_eq!(
        body,
        json!({"error": "budget_exceeded", "spentUsd": 5, "limitUsd": 5})
    );
}

#[tokio::test]
async fn match_is_503_when_official_lists_are_missing() {
    // it("공식 목록이 아직 없으면 503 listings_unavailable")
    let server = start_with(
        stock_llm(),
        Options {
            seeded: false,
            ..Options::default()
        },
    )
    .await;
    let (status, body) = server.post("/stocks/match", json!({"query": "삼전"})).await;
    assert_eq!(status, 503);
    assert_eq!(body, json!({"error": "listings_unavailable"}));
}

async fn llm_error_maps_to(kind: LlmErrorKind, name: &str, expected: u16) {
    let server = start_with(FailingLlm(kind), Options::default()).await;
    let (status, body) = server.post("/stocks/match", json!({"query": "삼전"})).await;
    assert_eq!(status, expected);
    assert_eq!(body, json!({"error": format!("llm_{name}")}));
}

#[tokio::test]
async fn llm_error_no_api_key_is_503() {
    // it.each LLM 오류 no_api_key → 503
    llm_error_maps_to(LlmErrorKind::NoApiKey, "no_api_key", 503).await;
}

#[tokio::test]
async fn llm_error_payment_required_is_402() {
    // it.each LLM 오류 payment_required → 402
    llm_error_maps_to(LlmErrorKind::PaymentRequired, "payment_required", 402).await;
}

#[tokio::test]
async fn llm_error_rate_limited_is_429() {
    // it.each LLM 오류 rate_limited → 429
    llm_error_maps_to(LlmErrorKind::RateLimited, "rate_limited", 429).await;
}

#[tokio::test]
async fn llm_error_timeout_is_504() {
    // it.each LLM 오류 timeout → 504
    llm_error_maps_to(LlmErrorKind::Timeout, "timeout", 504).await;
}

#[tokio::test]
async fn llm_error_invalid_output_is_502() {
    // it.each LLM 오류 invalid_output → 502
    llm_error_maps_to(LlmErrorKind::InvalidOutput, "invalid_output", 502).await;
}

#[tokio::test]
async fn llm_error_upstream_is_502() {
    // Rust 전용: TS의 it.each는 upstream을 빼놓았지만 표(LLM_ERROR_STATUS)에는 502로 있다.
    llm_error_maps_to(LlmErrorKind::Upstream, "upstream", 502).await;
}

// ---- describe("worker 내부 API: 등록·삭제") ----

#[tokio::test]
async fn register_conflict_delete_and_restore() {
    // it("POST /stocks: 201 등록 → 409 중복 → DELETE 200 → 다시 등록 200 복원")
    let server = start().await;
    let input =
        json!({"market": "US", "code": "NVDA", "userInput": "엔비디아", "nameKo": "엔비디아"});

    let (status, created) = server.post("/stocks", input.clone()).await;
    assert_eq!(status, 201);
    let id = created["stock"]["id"].as_i64().unwrap();
    assert_eq!(created["status"], "created");

    assert_eq!(server.post("/stocks", input.clone()).await.0, 409);

    assert_eq!(
        server.plain("DELETE", &format!("/stocks/{id}")).await.0,
        200
    );
    assert_eq!(
        list_active_stocks(&server.shared.db.lock().unwrap())
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        server.plain("DELETE", &format!("/stocks/{id}")).await.0,
        404
    );

    let (status, restored) = server.post("/stocks", input).await;
    assert_eq!(status, 200);
    assert_eq!(restored["status"], "restored");

    // 등록과 복원 때만 키워드를 새로 만든다(중복 등록은 아님, D-37).
    assert_eq!(server.queue.list(), vec![id, id]);
}

#[tokio::test]
async fn register_unlisted_stock_is_404_not_listed() {
    // it("공식 목록에 없으면 404 not_listed")
    let server = start().await;
    let (status, body) = server
        .post(
            "/stocks",
            json!({"market": "KR", "code": "123456", "userInput": "x"}),
        )
        .await;
    assert_eq!(status, 404);
    assert_eq!(body, json!({"error": "not_listed"}));
}

#[tokio::test]
async fn register_response_has_the_stock_row_shape_of_ts() {
    // Rust 전용: 등록 응답의 stock은 drizzle 행을 JSON으로 쓴 모양(키 이름, 시각은 ISO 글자)이다.
    let server = start().await;
    let (_, created) = server
        .post(
            "/stocks",
            json!({"market": "KR", "code": "005930", "userInput": "삼전"}),
        )
        .await;
    let stock = &created["stock"];
    let keys: Vec<&str> = stock
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        vec![
            "id",
            "market",
            "code",
            "name",
            "nameKo",
            "exchange",
            "userInput",
            "createdAt",
            "deletedAt",
            "keywordsStatus",
            "keywordsError",
            "keywordsGeneratedAt",
            "backfillFrom",
            "kind",
            "topicId"
        ]
    );
    assert_eq!(stock["market"], "KR");
    assert_eq!(stock["deletedAt"], Value::Null);
    assert_eq!(stock["keywordsStatus"], "pending");
    assert_eq!(stock["kind"], "stock");
    let created_at = stock["createdAt"].as_str().unwrap();
    assert!(
        created_at.ends_with('Z') && created_at.len() == 24,
        "{created_at}"
    );
}

#[test]
fn iso_writes_js_date_json_text() {
    // Rust 전용: JS `Date.toJSON()`과 같은 글자
    assert_eq!(
        iso(ms("2026-10-21T00:00:00Z") + 7),
        "2026-10-21T00:00:00.007Z"
    );
}

// ---- describe("worker 내부 API: 키워드 다시 만들기") ----

async fn registered() -> (Started, i64) {
    let server = start().await;
    let (_, body) = server
        .post(
            "/stocks",
            json!({"market": "KR", "code": "005930", "userInput": "삼전"}),
        )
        .await;
    let id = body["stock"]["id"].as_i64().unwrap();
    server.queue.clear();
    (server, id)
}

#[tokio::test]
async fn keywords_rebuild_for_one_stock_is_202_and_queued() {
    // it("POST /stocks/:id/keywords → 202, 대기열에 넣음")
    let (server, id) = registered().await;
    let (status, body) = server
        .post(&format!("/stocks/{id}/keywords"), json!(null))
        .await;
    assert_eq!(status, 202);
    assert_eq!(body, json!({"status": "queued"}));
    assert_eq!(server.queue.list(), vec![id]);
}

#[tokio::test]
async fn keywords_rebuild_404_for_missing_or_deleted_and_401_without_token() {
    // it("없거나 삭제한 종목은 404, 토큰이 없으면 401")
    let (server, id) = registered().await;
    assert_eq!(
        server
            .plain("POST", &format!("/stocks/{}/keywords", id + 1))
            .await
            .0,
        404
    );
    server.plain("DELETE", &format!("/stocks/{id}")).await;
    assert_eq!(
        server
            .plain("POST", &format!("/stocks/{id}/keywords"))
            .await
            .0,
        404
    );
    let (status, _) = server
        .call("POST", &format!("/stocks/{id}/keywords"), None, None)
        .await;
    assert_eq!(status, 401);
    assert_eq!(server.queue.list(), Vec::<i64>::new());
}

#[tokio::test]
async fn alias_remove_drops_the_alias_and_remembers_it() {
    let (server, id) = registered().await;
    {
        let conn = server.shared.db.lock().unwrap();
        conn.execute(
            "UPDATE stocks SET aliases = '[\"삼전\",\"Samsung\"]' WHERE id = ?1",
            [id],
        )
        .unwrap();
    }
    let (status, body) = server
        .post(
            &format!("/stocks/{id}/aliases/remove"),
            json!({"alias": "삼전"}),
        )
        .await;
    assert_eq!((status, body), (200, json!({"ok": true})));
    let stock = list_active_stocks(&server.shared.db.lock().unwrap())
        .unwrap()
        .remove(0);
    assert_eq!(stock.aliases, vec!["Samsung".to_string()]);
    assert_eq!(stock.aliases_removed, vec!["삼전".to_string()]);
    // 이미 지운 별칭은 404 alias_not_found
    let (status, body) = server
        .post(
            &format!("/stocks/{id}/aliases/remove"),
            json!({"alias": "삼전"}),
        )
        .await;
    assert_eq!((status, body), (404, json!({"error": "alias_not_found"})));
}

#[tokio::test]
async fn alias_remove_answers_404_for_missing_stock_400_for_bad_body_401_without_token() {
    let (server, id) = registered().await;
    let (status, body) = server
        .post(
            &format!("/stocks/{}/aliases/remove", id + 1),
            json!({"alias": "삼전"}),
        )
        .await;
    assert_eq!((status, body), (404, json!({"error": "not_found"})));
    let (status, _) = server
        .post(&format!("/stocks/{id}/aliases/remove"), json!({"alias": 3}))
        .await;
    assert_eq!(status, 400);
    let (status, _) = server
        .post(
            &format!("/stocks/{id}/aliases/remove"),
            json!({"alias": ""}),
        )
        .await;
    assert_eq!(status, 400);
    let (status, _) = server
        .call(
            "POST",
            &format!("/stocks/{id}/aliases/remove"),
            Some(Body_::Json(json!({"alias": "삼전"}))),
            None,
        )
        .await;
    assert_eq!(status, 401);
}

// ---- describe("worker 내부 API: 기사 수집") ----

#[tokio::test]
async fn collect_is_202_with_run_id_and_409_while_running() {
    // it("POST /collect → 202와 회차 id, 도는 중에 다시 부르면 409")
    let server = start().await;
    let (status, first) = server.plain("POST", "/collect").await;
    assert_eq!(status, 202);
    assert_eq!(first, json!({"runId": 1}));
    assert_eq!(server.collector.started(), vec!["manual"]);

    let (status, again) = server.plain("POST", "/collect").await;
    assert_eq!(status, 409);
    assert_eq!(again, json!({"error": "already_running", "runId": 1}));

    server.collector.finish();
    assert_eq!(server.plain("POST", "/collect").await.0, 202);
}

#[tokio::test]
async fn collect_without_token_is_401() {
    // it("토큰이 없으면 401")
    let server = start().await;
    assert_eq!(server.call("POST", "/collect", None, None).await.0, 401);
    assert_eq!(server.collector.started(), Vec::<&str>::new());
}

// ---- describe("worker 내부 API: 설정 (SPEC FR-8)") ----

fn form() -> Value {
    json!({
        "collectIntervalMinutes": "60",
        "keywordsDepth": "1",
        "keywordsRegenerateEvery": "daily",
        "minRelevance": "6.5",
        "minMagnitude": "5",
        "keywordMinRelevance": "7",
        "keywordMinMagnitude": "5",
        "topicIntervalHours": "4",
        "topicTimes": "15:40, 8:30,08:30",
        "maxPerRun": "150",
        "monthlyBudgetUsd": "7.5",
        "llmModel": "anthropic/claude-haiku-5.5",
    })
}

fn with_form(changes: Value) -> Value {
    let mut body = form();
    for (key, value) in changes.as_object().unwrap() {
        body[key] = value.clone();
    }
    body
}

#[tokio::test]
async fn settings_save_checked_values_at_once_and_report_changes() {
    // it("PUT /settings: 검사한 값을 한 번에 저장하고 바뀐 항목을 알려 준다")
    let checked = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen = checked.clone();
    let server = start_with(
        stock_llm(),
        Options {
            check_model: Arc::new(move |id| {
                seen.lock().unwrap().push(id);
                Box::pin(async { Ok(true) })
            }),
            ..Options::default()
        },
    )
    .await;
    let (status, body) = server.put("/settings", form()).await;
    assert_eq!(status, 200);
    assert_eq!(
        body,
        json!({"ok": true, "changed": [
            "collectIntervalMinutes", "keywordsDepth", "keywordsRegenerateEvery", "minRelevance",
            "keywordMinRelevance", "topicIntervalHours", "topicTimes", "maxPerRun", "monthlyBudgetUsd"
        ]})
    );
    let saved = read_settings_form(&server.shared.db.lock().unwrap()).unwrap();
    assert_eq!(saved.collect_interval_minutes, 60.0);
    assert_eq!(saved.keywords_depth, 1.0);
    assert_eq!(saved.keywords_regenerate_every.as_str(), "daily");
    assert_eq!(saved.min_relevance, 6.5);
    assert_eq!(saved.min_magnitude, 5.0);
    assert_eq!(saved.keyword_min_relevance, 7.0);
    assert_eq!(saved.keyword_min_magnitude, 5.0);
    assert_eq!(saved.topic_interval_hours, 4.0);
    // 한 자리 시각은 두 자리로, 겹치는 시각은 하나로, 시각 순으로
    assert_eq!(saved.topic_times, vec!["08:30", "15:40"]);
    assert_eq!(saved.max_per_run, 150.0);
    assert_eq!(saved.monthly_budget_usd, 7.5);
    assert_eq!(saved.llm_model, "anthropic/claude-haiku-5.5");
    // 모델을 바꾸지 않았으면 OpenRouter에 묻지 않는다.
    assert!(checked.lock().unwrap().is_empty());
}

#[tokio::test]
async fn settings_invalid_values_are_400_with_reasons_and_nothing_saved() {
    // it("값이 틀리면 400과 항목별 이유, 아무것도 저장하지 않는다")
    let server = start().await;
    let (status, body) = server
        .put(
            "/settings",
            with_form(json!({
                "collectIntervalMinutes": "45",
                "minRelevance": "11",
                "minMagnitude": "-1",
                "topicIntervalHours": "1.5",
                "topicTimes": "09:00, 25:00",
                "llmModel": "모델",
            })),
        )
        .await;
    assert_eq!(status, 400);
    assert_eq!(
        body,
        json!({"error": "invalid_settings", "fields": {
            "collectIntervalMinutes": "15·30·60·180분 중에서 고르세요.",
            "minRelevance": "0~10 사이로 넣으세요.",
            "minMagnitude": "0~10 사이로 넣으세요.",
            "topicIntervalHours": "0~24 사이 정수로 넣으세요.",
            "topicTimes": "09:00, 15:30처럼 24시간제 HH:MM으로 넣으세요.",
            "llmModel": "모델 ID 형식이 아닙니다(예: anthropic/claude-haiku-5.5).",
        }})
    );
    let saved = read_settings_form(&server.shared.db.lock().unwrap()).unwrap();
    assert_eq!(saved.collect_interval_minutes, 30.0);
}

#[tokio::test]
async fn settings_model_change_is_checked_against_openrouter_list() {
    // it("모델을 바꾸면 OpenRouter 목록으로 확인하고, 없거나 확인하지 못하면 저장하지 않는다")
    let body = with_form(json!({"llmModel": "openai/gpt-none"}));
    let collect = |server: &Started| {
        read_settings_form(&server.shared.db.lock().unwrap())
            .unwrap()
            .collect_interval_minutes
    };

    let unknown = start_with(
        stock_llm(),
        Options {
            check_model: always(false),
            ..Options::default()
        },
    )
    .await;
    let (status, payload) = unknown.put("/settings", body.clone()).await;
    assert_eq!(status, 422);
    assert_eq!(payload, json!({"error": "unknown_model"}));
    assert_eq!(collect(&unknown), 30.0);

    let offline = start_with(
        stock_llm(),
        Options {
            check_model: Arc::new(|_| {
                Box::pin(async { Err(ModelListError("연결 실패".to_string())) })
            }),
            ..Options::default()
        },
    )
    .await;
    let (status, payload) = offline.put("/settings", body.clone()).await;
    assert_eq!(status, 502);
    assert_eq!(payload, json!({"error": "model_check_failed"}));

    let known = start_with(
        stock_llm(),
        Options {
            check_model: always(true),
            ..Options::default()
        },
    )
    .await;
    assert_eq!(known.put("/settings", body).await.0, 200);
    let saved = read_settings_form(&known.shared.db.lock().unwrap()).unwrap();
    assert_eq!(saved.llm_model, "openai/gpt-none");
}

#[tokio::test]
async fn settings_with_non_object_body_is_400_with_no_fields() {
    // Rust 전용: 본문이 JSON이 아니면 TS도 `safeParse(null)`이 실패해 fields가 빈 400이다.
    let server = start().await;
    let (status, body) = server
        .call(
            "PUT",
            "/settings",
            Some(Body_::Text("{not json".to_string())),
            Some(TOKEN),
        )
        .await;
    assert_eq!(status, 400);
    assert_eq!(body, json!({"error": "invalid_settings", "fields": {}}));
}

#[tokio::test]
async fn keywords_rebuild_only_stocks_with_other_depth() {
    // it("POST /keywords/rebuild: 저장한 깊이와 다른 깊이로 만든 종목만 다시 만든다")
    let server = start().await;
    let register = |code: &str, query: &str| {
        let conn = server.shared.db.lock().unwrap();
        match register_stock(
            &conn,
            &RegisterInput {
                market: Market::Kr,
                code,
                user_input: query,
                name_ko: None,
            },
            ms("2026-10-21T00:00:00Z"),
        )
        .unwrap()
        {
            RegisterResult::Created(stock) => stock.id,
            _ => panic!("표본에 없음"),
        }
    };
    let deep = register("005930", "삼전");
    let fresh = register("000660", "하닉");
    let node = |key: i64, level: i64, parent_key: Option<i64>| KeywordNode {
        key,
        parent_key,
        level,
        kind: KeywordKind::Structure,
        text_ko: format!("키워드 {key}"),
        text_en: None,
        reason: "r".to_string(),
        path: format!("경로 {key}"),
        relevance: None,
        magnitude: None,
    };
    let at = ms("2026-10-21T00:00:00Z");
    {
        let mut conn = server.shared.db.lock().unwrap();
        // 삼성전자는 깊이 1, SK하이닉스는 깊이 2(기본)로 만들어져 있다.
        replace_keywords(&mut conn, deep, &[node(1, 1, None)], at).unwrap();
        replace_keywords(
            &mut conn,
            fresh,
            &[node(1, 1, None), node(2, 2, Some(1))],
            at,
        )
        .unwrap();
    }
    let (status, body) = server.plain("POST", "/keywords/rebuild").await;
    assert_eq!(status, 202);
    assert_eq!(body, json!({"count": 1}));
    assert_eq!(server.queue.list(), vec![deep]);
}

// ---- describe("worker 내부 API: 시장·주제 (11단계)") ----

#[tokio::test]
async fn market_toggle_creates_keywords_when_on_and_hides_when_off() {
    // it("PUT /markets/:market: 증시를 켜면 키워드를 만들고, 끄면 숨긴다")
    let server = start().await;
    let kr = ensure_default_market(&server.shared.db.lock().unwrap(), now_ms())
        .unwrap()
        .unwrap();

    let (status, _) = server.put("/markets/US", json!({"enabled": true})).await;
    assert_eq!(status, 200);
    let markets = list_market_items(&server.shared.db.lock().unwrap()).unwrap();
    assert_eq!(
        markets
            .iter()
            .map(|m| (m.market, m.stock.is_some()))
            .collect::<Vec<_>>(),
        vec![(Market::Kr, true), (Market::Us, true)]
    );
    assert_eq!(
        server.queue.list(),
        vec![markets[1].stock.as_ref().unwrap().id]
    );

    assert_eq!(
        server.put("/markets/KR", json!({"enabled": false})).await.0,
        200
    );
    assert!(
        list_market_items(&server.shared.db.lock().unwrap()).unwrap()[0]
            .stock
            .is_none()
    );
    // 다시 켜면 같은 항목을 되살리고 트리를 새로 만든다.
    server.put("/markets/KR", json!({"enabled": true})).await;
    let revived = list_market_items(&server.shared.db.lock().unwrap()).unwrap();
    assert_eq!(revived[0].stock.as_ref().unwrap().id, kr);
    assert_eq!(*server.queue.list().last().unwrap(), kr);

    assert_eq!(
        server.put("/markets/JP", json!({"enabled": true})).await.0,
        404
    );
    assert_eq!(
        server.put("/markets/US", json!({"enabled": "yes"})).await.0,
        400
    );
}

#[tokio::test]
async fn topics_add_builds_trees_per_market_and_delete_hides() {
    // it("POST /topics·DELETE /topics/:id: 주제를 넣으면 켜진 증시마다 트리를 만들고, 지우면 숨긴다")
    let server = start().await;
    ensure_default_market(&server.shared.db.lock().unwrap(), now_ms()).unwrap();

    let (status, added) = server
        .post("/topics", json!({"name": " 부동산  시장 "}))
        .await;
    assert_eq!(status, 201);
    let topic_id = added["topicId"].as_i64().unwrap();
    let items = list_user_topic_items(&server.shared.db.lock().unwrap()).unwrap();
    assert_eq!(items[0].topic.id, topic_id);
    assert_eq!(items[0].topic.name, "부동산 시장");
    assert_eq!(
        server.queue.list(),
        items[0]
            .lenses
            .iter()
            .map(|lens| lens.id)
            .collect::<Vec<_>>()
    );

    let (status, again) = server.post("/topics", json!({"name": "부동산시장"})).await;
    assert_eq!(status, 409);
    assert_eq!(again, json!({"error": "exists"}));
    let (status, short) = server.post("/topics", json!({"name": "가"})).await;
    assert_eq!(status, 400);
    assert_eq!(short, json!({"error": "invalid"}));

    assert_eq!(
        server
            .plain("DELETE", &format!("/topics/{topic_id}"))
            .await
            .0,
        200
    );
    assert!(
        list_user_topic_items(&server.shared.db.lock().unwrap())
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        server
            .plain("DELETE", &format!("/topics/{topic_id}"))
            .await
            .0,
        404
    );
}

fn read_count(server: &Started) -> i64 {
    server
        .shared
        .db
        .lock()
        .unwrap()
        .query_row("SELECT count(*) FROM article_reads", [], |row| row.get(0))
        .unwrap()
}

#[tokio::test]
async fn articles_read_marks_and_unmarks_grouped_articles() {
    // it("PUT /articles/read: 묶인 기사들을 읽음·안 읽음으로 저장한다(12단계)")
    let server = start().await;
    let ids: Vec<i64> = {
        let conn = server.shared.db.lock().unwrap();
        let stock = match register_stock(
            &conn,
            &RegisterInput {
                market: Market::Kr,
                code: "005930",
                user_input: "삼성전자",
                name_ko: None,
            },
            ms("2026-10-21T00:00:00Z"),
        )
        .unwrap()
        {
            RegisterResult::Created(stock) => stock,
            _ => panic!("표본에 없음"),
        };
        [1, 2]
            .iter()
            .map(|hour| {
                let mut link = LinkSpec::new(
                    stock.id,
                    "기사",
                    ms("2026-10-28T00:00:00Z") + hour * 60 * 60 * 1000,
                );
                link.relevance = None;
                insert_link(&conn, &link).1
            })
            .collect()
    };
    let put = |body: Value| server.put("/articles/read", body);

    let (status, marked) = put(json!({"articleIds": [ids[0], ids[1], 9999], "read": true})).await;
    assert_eq!(status, 200);
    assert_eq!(marked, json!({"changed": 2}));
    assert_eq!(read_count(&server), 2);

    let (_, unmarked) = put(json!({"articleIds": [ids[0]], "read": false})).await;
    assert_eq!(unmarked, json!({"changed": 1}));
    let remaining: i64 = server
        .shared
        .db
        .lock()
        .unwrap()
        .query_row("SELECT article_id FROM article_reads", [], |row| row.get(0))
        .unwrap();
    assert_eq!(remaining, ids[1]);

    // 로그인 토큰이 없으면 401, 본문이 틀리면 400이고 저장하지 않는다.
    let (status, _) = server
        .call(
            "PUT",
            "/articles/read",
            Some(Body_::Json(json!({"articleIds": ids, "read": true}))),
            None,
        )
        .await;
    assert_eq!(status, 401);
    let too_many: Vec<i64> = (1..=201).collect();
    for body in [
        json!({}),
        json!({"articleIds": [], "read": true}),
        json!({"articleIds": [0], "read": true}),
        json!({"articleIds": [1.5], "read": true}),
        json!({"articleIds": ["1"], "read": true}),
        json!({"articleIds": too_many, "read": true}),
        json!({"articleIds": ids, "read": "yes"}),
    ] {
        assert_eq!(put(body).await.0, 400);
    }
    assert_eq!(read_count(&server), 1);
}

#[tokio::test]
async fn topics_refresh_starts_and_returns_and_503_without_refresher() {
    // it("POST /topics/refresh: 토픽 갱신을 시작하고 바로 돌아온다(이미 하고 있으면 started false)")
    let refresher = Arc::new(FakeRefresher::default());
    let server = start_with(
        stock_llm(),
        Options {
            refresher: Some(refresher.clone()),
            ..Options::default()
        },
    )
    .await;
    let (status, first) = server.plain("POST", "/topics/refresh").await;
    assert_eq!(status, 202);
    assert_eq!(first, json!({"started": true}));
    let (_, second) = server.plain("POST", "/topics/refresh").await;
    assert_eq!(second, json!({"started": false}));
    assert_eq!(*refresher.calls.lock().unwrap(), 2);

    let without = start().await;
    assert_eq!(without.plain("POST", "/topics/refresh").await.0, 503);
    assert_eq!(
        read_num(&without.shared.db.lock().unwrap(), &TOPICS_REFRESHED_AT).unwrap(),
        0.0
    );
}

#[tokio::test]
async fn wrong_method_on_known_path_is_404() {
    // Rust 전용: TS는 (메서드, 주소)가 모두 맞아야 처리한다. 주소만 맞으면 인증 뒤 404다.
    let server = start().await;
    let (status, body) = server.plain("GET", "/stocks").await;
    assert_eq!(status, 404);
    assert_eq!(body, json!({"error": "not_found"}));
    assert_eq!(server.plain("GET", "/collect").await.0, 404);
}
