use serde_json::{Value, json};

use super::*;

#[test]
fn budget_exceeded_includes_limit() {
    // it("예산 초과면 한도를 함께 알려 준다")
    assert_eq!(
        describe_stock_error("budget_exceeded", &json!({ "limitUsd": 5 })),
        "이번 달 AI 예산 $5을 모두 써서 지금은 찾을 수 없습니다."
    );
}

#[test]
fn missing_key_tells_where_to_fix() {
    // it("키가 없으면 어디를 고쳐야 하는지 알려 준다")
    assert!(describe_stock_error("llm_no_api_key", &Value::Null).contains("OPENROUTER_API_KEY"));
}

#[test]
fn unknown_code_still_becomes_a_sentence() {
    // it("모르는 코드도 문장으로 바꾼다")
    assert_eq!(
        describe_stock_error("something_new", &Value::Null),
        "처리하지 못했습니다. 다시 시도하세요."
    );
}

#[test]
fn limit_is_written_like_a_js_number_and_only_when_numeric() {
    // Rust에서만 생기는 경우: `${body.limitUsd}`는 JS 숫자 글자, 숫자가 아니면 한도를 생략
    assert_eq!(
        describe_stock_error("budget_exceeded", &json!({ "limitUsd": 0.5 })),
        "이번 달 AI 예산 $0.5을 모두 써서 지금은 찾을 수 없습니다."
    );
    assert_eq!(
        describe_stock_error("budget_exceeded", &json!({ "limitUsd": "5" })),
        "이번 달 AI 예산을 모두 써서 지금은 찾을 수 없습니다."
    );
}

#[test]
fn every_known_code_has_its_own_sentence() {
    let fallback = "처리하지 못했습니다. 다시 시도하세요.";
    for code in [
        "llm_no_api_key",
        "llm_payment_required",
        "llm_rate_limited",
        "llm_timeout",
        "llm_upstream",
        "llm_invalid_output",
        "listings_unavailable",
        "already_registered",
        "not_listed",
        "not_found",
        "invalid",
        "exists",
        "unavailable",
        "worker_timeout",
        "worker_unreachable",
    ] {
        assert_ne!(describe_stock_error(code, &Value::Null), fallback, "{code}");
    }
}
