use serde_json::{Value, json};

use super::*;
use crate::server::ai::llm::LlmErrorKind;
use crate::server::ai::llm::LlmUsage;
use crate::server::settings::{LLM_MODEL as MODEL_SETTING, write_str};
use crate::server::stocks::repository::{RegisterInput, register_stock};
use crate::test_support::{SharedTestDb, fake_llm, seed_listings, shared_test_db};

const USAGE: LlmUsage = LlmUsage {
    input_tokens: 500,
    output_tokens: 60,
    cost_usd: 0.0001,
};
const NOW: EpochMs = 1_792_000_000_000;

fn seeded() -> SharedTestDb {
    let shared = shared_test_db();
    seed_listings(&mut shared.db.lock().unwrap());
    shared
}

fn kr(code: &str, name: &str) -> LlmCandidate {
    LlmCandidate {
        market: Market::Kr,
        code: code.into(),
        name: name.into(),
        name_ko: name.into(),
    }
}

fn us(code: &str, name: &str, name_ko: &str) -> LlmCandidate {
    LlmCandidate {
        market: Market::Us,
        code: code.into(),
        name: name.into(),
        name_ko: name_ko.into(),
    }
}

fn candidates_json(candidates: &[LlmCandidate]) -> Value {
    json!({
        "candidates": candidates.iter().map(|c| json!({
            "market": c.market.as_str(), "code": c.code, "name": c.name, "nameKo": c.name_ko,
        })).collect::<Vec<_>>()
    })
}

// describe("verifyCandidates (SPEC D-12: 공식 목록과 대조)")
#[test]
fn keeps_listed_candidates_and_uses_listing_name_and_exchange() {
    // it("목록에 있는 후보만 남기고, 이름·거래소는 목록 값을 쓴다")
    let shared = seeded();
    let conn = shared.db.lock().unwrap();
    let result = verify_candidates(
        &conn,
        &[
            kr("005930", "삼성전자(주)"),
            us("NVDA", "Nvidia", "엔비디아"),
            kr("999999", "없는회사"),
        ],
    )
    .unwrap();
    assert_eq!(
        result,
        [
            StockCandidate {
                market: Market::Kr,
                code: "005930".into(),
                name: "삼성전자".into(),
                name_ko: Some("삼성전자".into()),
                exchange: "KOSPI".into(),
                is_etf: false,
                already_registered: false,
            },
            StockCandidate {
                market: Market::Us,
                code: "NVDA".into(),
                name: "NVIDIA Corporation - Common Stock".into(),
                name_ko: Some("엔비디아".into()),
                exchange: "NASDAQ".into(),
                is_etf: false,
                already_registered: false,
            },
        ]
    );
}

#[test]
fn korean_candidate_with_wrong_code_is_fixed_by_official_name() {
    // it("한국 종목은 코드가 틀려도 정식 이름이 맞으면 목록의 코드로 바로잡는다")
    let shared = seeded();
    let conn = shared.db.lock().unwrap();
    let result = verify_candidates(&conn, &[kr("000000", "SK하이닉스")]).unwrap();
    assert_eq!(result[0].code, "000660");
}

#[test]
fn code_notation_differences_are_matched() {
    // it("코드 표기 차이(A 접두어, 하이픈 클래스)를 맞춘다")
    let shared = seeded();
    let conn = shared.db.lock().unwrap();
    let result = verify_candidates(
        &conn,
        &[
            kr("A005930", "삼성전자"),
            us("BRK-B", "Berkshire Hathaway", "버크셔 해서웨이"),
        ],
    )
    .unwrap();
    assert_eq!(
        result.iter().map(|c| c.code.as_str()).collect::<Vec<_>>(),
        ["005930", "BRK.B"]
    );
}

#[test]
fn same_stock_once_and_at_most_three() {
    // it("같은 종목은 한 번만, 최대 3개까지")
    let shared = seeded();
    let conn = shared.db.lock().unwrap();
    let result = verify_candidates(
        &conn,
        &[
            kr("005930", "삼성전자"),
            kr("005930", "삼성전자"),
            kr("000660", "SK하이닉스"),
            kr("000720", "현대건설"),
            kr("035720", "카카오"),
        ],
    )
    .unwrap();
    assert_eq!(result.len(), MAX_CANDIDATES);
    assert_eq!(
        result.iter().map(|c| c.code.as_str()).collect::<Vec<_>>(),
        ["005930", "000660", "000720"]
    );
}

#[test]
fn already_registered_stock_is_marked() {
    // it("이미 등록한 종목은 표시한다")
    let shared = seeded();
    let conn = shared.db.lock().unwrap();
    register_stock(
        &conn,
        &RegisterInput {
            market: Market::Kr,
            code: "005930",
            user_input: "삼전",
            name_ko: None,
        },
        NOW,
    )
    .unwrap();
    assert!(verify_candidates(&conn, &[kr("005930", "삼성전자")]).unwrap()[0].already_registered);
}

// describe("matchStocks")
#[tokio::test(flavor = "current_thread")]
async fn calls_with_setting_model_and_records_stock_match_usage() {
    // it("설정의 모델로 부르고, 사용량을 stock_match로 기록한다")
    let shared = seeded();
    write_str(
        &shared.db.lock().unwrap(),
        &MODEL_SETTING,
        "google/gemini-3.8-flash",
        NOW,
    )
    .unwrap();
    let llm = fake_llm(|_| candidates_json(&[kr("005930", "삼성전자")]), USAGE);

    let candidates = match_stocks(&shared.db, &llm, "  삼전 ", None)
        .await
        .unwrap();

    assert_eq!(
        candidates
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["삼성전자"]
    );
    let calls = llm.calls.lock().unwrap();
    assert_eq!(calls[0].model, "google/gemini-3.8-flash");
    assert_eq!(calls[0].messages.last().unwrap(), &LlmMessage::user("삼전"));
    assert_eq!(calls[0].schema_name, "stock_candidates");
    assert_eq!(calls[0].max_tokens, 2000);
    let purpose: String = shared
        .db
        .lock()
        .unwrap()
        .query_row("SELECT purpose FROM usage_logs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(purpose, "stock_match");
}

#[tokio::test(flavor = "current_thread")]
async fn without_any_listing_fails_without_calling_llm() {
    // it("공식 목록이 하나도 없으면 LLM을 부르지 않고 실패한다")
    let shared = shared_test_db();
    let llm = fake_llm(|_| json!({ "candidates": [] }), USAGE);
    let error = match_stocks(&shared.db, &llm, "삼전", None)
        .await
        .unwrap_err();
    assert!(matches!(error, MatchError::Listings(_)), "{error}");
    assert_eq!(error.to_string(), "공식 종목 목록을 아직 받지 못했습니다.");
    assert_eq!(llm.call_count(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn wrong_response_shape_is_invalid_output_and_still_records_cost() {
    // Rust에서만 생기는 경우: zod 검증을 대신하는 parse 함수의 경계(필드 빠짐·시장 값 틀림)
    for bad in [
        json!({ "candidates": [{ "market": "KR", "code": "005930", "name": "삼성전자" }] }),
        json!({ "candidates": [{ "market": "JP", "code": "1", "name": "x", "nameKo": "x" }] }),
        json!({ "candidates": null }),
        json!({}),
    ] {
        let shared = seeded();
        let llm = fake_llm(move |_| bad.clone(), USAGE);
        let error = match_stocks(&shared.db, &llm, "삼전", None)
            .await
            .unwrap_err();
        match error {
            MatchError::Llm(e) => assert_eq!(e.kind, LlmErrorKind::InvalidOutput),
            other => panic!("{other}"),
        }
        let logs: i64 = shared
            .db
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM usage_logs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(logs, 1);
    }
}

const PROMPT_FIXTURE: &str = include_str!("../prompts/match.json");

#[test]
fn match_prompt_equals_ts_prompt_character_by_character() {
    // 프롬프트는 TS와 글자까지 같아야 한다(`buildMatchMessages`, TS가 내보낸 파일과 비교).
    let fixture: Value = serde_json::from_str(PROMPT_FIXTURE).unwrap();
    for entry in fixture.as_array().unwrap() {
        let query = entry["query"].as_str().unwrap();
        let expected: Vec<(String, String)> = entry["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| {
                (
                    m["role"].as_str().unwrap().to_string(),
                    m["content"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        let actual: Vec<(String, String)> = build_match_messages(query)
            .into_iter()
            .map(|m| (m.role.to_string(), m.content))
            .collect();
        assert_eq!(actual, expected, "{query}");
    }
}
