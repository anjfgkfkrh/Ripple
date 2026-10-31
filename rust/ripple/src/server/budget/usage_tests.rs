use std::sync::Mutex;

use serde_json::{Value, json};

use super::*;
use crate::server::ai::decider::types::{
    DecisionInput, JudgeItem, JudgeTarget, LeadSubject, LeadSubjectType, TokenUsage,
};
use crate::server::ai::llm::{LlmErrorKind, LlmMessage};
use crate::server::db::schema::Market;
use crate::server::settings::{BUDGET_MONTHLY_USD, write_num};
use crate::test_support::{fake_llm, ms, shared_test_db, test_db};

fn parse_ok(value: &Value) -> Option<bool> {
    value.get("ok")?.as_bool()
}

fn request() -> StructuredRequest<bool> {
    StructuredRequest {
        model: "test/model".into(),
        messages: vec![LlmMessage::user("hi")],
        schema: json!({ "type": "object" }),
        schema_name: "ok".into(),
        max_tokens: 100,
        timeout_ms: None,
        parse: parse_ok,
    }
}

fn usage(cost_usd: f64) -> LlmUsage {
    LlmUsage {
        input_tokens: 10,
        output_tokens: 5,
        cost_usd,
    }
}

// 한국 시간 2026-10-20 12:00
fn now() -> EpochMs {
    ms("2026-10-20T03:00:00Z")
}

fn log(conn: &Connection, at: EpochMs, cost: f64) {
    record_usage(
        conn,
        &UsageEntry {
            provider: UsageProvider::Openrouter,
            model: "m",
            purpose: "test",
            usage: usage(cost),
            at,
            stock_id: None,
        },
    )
    .unwrap();
}

fn options(purpose: &str) -> CallOptions<'_> {
    CallOptions {
        purpose,
        now: Some(&now),
        stock_id: None,
    }
}

fn usage_logs(conn: &Connection) -> Vec<(String, String, String, i64, i64, f64)> {
    let mut stmt = conn
        .prepare("SELECT provider, model, purpose, input_tokens, output_tokens, cost_usd FROM usage_logs ORDER BY id")
        .unwrap();
    stmt.query_map([], |r| {
        Ok((
            r.get(0)?,
            r.get(1)?,
            r.get(2)?,
            r.get(3)?,
            r.get(4)?,
            r.get(5)?,
        ))
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

// describe("monthSpendUsd")
#[test]
fn month_spend_adds_only_this_month_in_seoul_time() {
    // it("이번 달(한국 시간) 기록만 더한다")
    let db = test_db();
    log(&db.conn, ms("2026-09-30T14:59:59Z"), 1.0); // 한국 9월 30일 23:59 → 지난달
    log(&db.conn, ms("2026-09-30T15:00:00Z"), 0.25); // 한국 10월 1일 00:00 → 이번 달
    log(&db.conn, ms("2026-10-19T10:00:00Z"), 0.5);

    let spent = month_spend_usd(&db.conn, now()).unwrap();
    assert!((spent - 0.75).abs() < 1e-9);
}

// describe("generateWithBudget (CLAUDE.md: 한도 검사 → 호출 → 기록)")
#[tokio::test(flavor = "current_thread")]
async fn within_limit_calls_and_records_usage() {
    // it("한도 안이면 호출하고 사용량을 기록한다")
    let shared = shared_test_db();
    let llm = fake_llm(|_| json!({ "ok": true }), usage(0.001));

    let result = generate_with_budget(&shared.db, &llm, &request(), &options("stock_match"))
        .await
        .unwrap();

    assert!(result.value);
    let conn = shared.db.lock().unwrap();
    assert_eq!(
        usage_logs(&conn),
        [(
            "openrouter".to_string(),
            "test/model".to_string(),
            "stock_match".to_string(),
            10,
            5,
            0.001
        )]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn at_limit_blocks_without_calling() {
    // it("이번 달 사용액이 한도(기본 $5)에 닿으면 호출하지 않고 막는다")
    let shared = shared_test_db();
    log(&shared.db.lock().unwrap(), now(), 5.0);
    let llm = fake_llm(|_| json!({ "ok": true }), usage(0.001));

    let error = generate_with_budget(&shared.db, &llm, &request(), &options("stock_match"))
        .await
        .unwrap_err();

    match error {
        GenerateError::Budget(e) => assert_eq!(
            e,
            BudgetExceededError {
                spent_usd: 5.0,
                limit_usd: 5.0
            }
        ),
        other => panic!("{other}"),
    }
    assert_eq!(llm.call_count(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn limit_follows_setting() {
    // it("한도는 설정 값을 따른다")
    let shared = shared_test_db();
    {
        let conn = shared.db.lock().unwrap();
        write_num(&conn, &BUDGET_MONTHLY_USD, 0.5, now()).unwrap();
        log(&conn, now(), 0.6);
    }
    let llm = fake_llm(|_| json!({ "ok": true }), usage(0.001));
    let error = generate_with_budget(&shared.db, &llm, &request(), &options("t"))
        .await
        .unwrap_err();
    assert!(matches!(error, GenerateError::Budget(_)), "{error}");
}

#[tokio::test(flavor = "current_thread")]
async fn records_billed_cost_even_when_response_shape_is_wrong() {
    // it("응답 형식이 틀려 실패해도 청구된 비용은 기록한다")
    let shared = shared_test_db();
    let llm = fake_llm(|_| json!({ "wrong": 1 }), usage(0.002));

    let error = generate_with_budget(&shared.db, &llm, &request(), &options("t"))
        .await
        .unwrap_err();

    match error {
        GenerateError::Llm(e) => assert_eq!(e.kind, LlmErrorKind::InvalidOutput),
        other => panic!("{other}"),
    }
    let conn = shared.db.lock().unwrap();
    assert_eq!(usage_logs(&conn)[0].5, 0.002);
}

#[test]
fn budget_exceeded_message_matches_ts() {
    // Rust에서만 필요한 점검: 화면에 나가는 문장은 toFixed(4)와 JS 숫자 글자를 쓴다.
    let error = BudgetExceededError {
        spent_usd: 5.0,
        limit_usd: 5.0,
    };
    assert_eq!(
        error.to_string(),
        "이번 달 AI 예산을 모두 썼습니다(사용 $5.0000 / 한도 $5)."
    );
    let error = BudgetExceededError {
        spent_usd: 0.03125,
        limit_usd: 0.5,
    };
    assert_eq!(
        error.to_string(),
        "이번 달 AI 예산을 모두 썼습니다(사용 $0.0313 / 한도 $0.5)."
    );
}

// rateWithBudget: TS에는 단독 테스트가 없어 Rust에서 한도 검사 → 호출 → 기록 순서를 고정한다.
struct FakeDecider {
    rating: LeadRating,
    models: Mutex<Vec<String>>,
}

impl Decider for FakeDecider {
    async fn decide(
        &self,
        _: &DecisionInput,
        _: &str,
    ) -> Result<crate::server::ai::decider::types::Decision, DeciderError> {
        unreachable!("이 테스트는 rate만 쓴다")
    }

    async fn rate(&self, _: &LeadInput, model: &str) -> Result<LeadRating, DeciderError> {
        self.models.lock().unwrap().push(model.to_string());
        Ok(self.rating.clone())
    }
}

fn lead_input() -> LeadInput {
    LeadInput {
        stock: JudgeItem {
            kind: JudgeTarget::Stock,
            name: "삼성전자".into(),
            official_name: None,
            market: Market::Kr,
            exchange: "KOSPI".into(),
            code: "005930".into(),
            summary: None,
        },
        subject: LeadSubject {
            kind: LeadSubjectType::Keyword,
            text: "HBM".into(),
            path: None,
            summary: None,
            terms: None,
            reason: None,
        },
    }
}

fn fake_decider() -> FakeDecider {
    FakeDecider {
        rating: LeadRating {
            relevance: 8.0,
            relevance_confidence: 0.8,
            magnitude: 6.0,
            magnitude_confidence: 0.5,
            model: "jev-1.13.0".into(),
            usage: TokenUsage {
                input_tokens: 1_000_000,
                output_tokens: 20,
            },
        },
        models: Mutex::new(Vec::new()),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn rate_records_input_tokens_times_unit_price() {
    let shared = shared_test_db();
    let decider = fake_decider();
    let rating = rate_with_budget(
        &shared.db,
        &decider,
        &lead_input(),
        &options("keyword_rating"),
    )
    .await
    .unwrap();
    assert_eq!(rating.relevance, 8.0);
    assert_eq!(decider.models.lock().unwrap().as_slice(), ["jev-1.13.0"]);
    let conn = shared.db.lock().unwrap();
    // 입력 100만 토큰 × 기본 단가 $0.042 / 100만
    assert_eq!(
        usage_logs(&conn),
        [(
            "typesafe".to_string(),
            "jev-1.13.0".to_string(),
            "keyword_rating".to_string(),
            1_000_000,
            20,
            0.042
        )]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn rate_is_blocked_at_limit_without_calling_decider() {
    let shared = shared_test_db();
    log(&shared.db.lock().unwrap(), now(), 5.0);
    let decider = fake_decider();
    let error = rate_with_budget(
        &shared.db,
        &decider,
        &lead_input(),
        &options("keyword_rating"),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, RateError::Budget(_)), "{error}");
    assert!(decider.models.lock().unwrap().is_empty());
}
