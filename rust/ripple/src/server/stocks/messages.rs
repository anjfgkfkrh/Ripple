use serde_json::Value;

use crate::server::text::js_number;

/// worker가 준 오류 코드를 화면에 보여 줄 문장으로 바꾼다. `body`는 오류 응답 본문(없으면 `Value::Null`).
pub fn describe_stock_error(error: &str, body: &Value) -> String {
    match error {
        "budget_exceeded" => {
            // typeof body.limitUsd === "number"
            let limit = body
                .get("limitUsd")
                .and_then(Value::as_f64)
                .map_or_else(String::new, |v| format!(" ${}", js_number(v)));
            format!("이번 달 AI 예산{limit}을 모두 써서 지금은 찾을 수 없습니다.")
        }
        "llm_no_api_key" => {
            "OpenRouter API 키가 설정되지 않았습니다. .env의 OPENROUTER_API_KEY를 확인하세요."
                .to_string()
        }
        "llm_payment_required" => {
            "OpenRouter 크레딧이 부족하거나 키의 사용 한도에 걸렸습니다.".to_string()
        }
        "llm_rate_limited" => "요청이 많아 잠시 막혔습니다. 조금 뒤에 다시 시도하세요.".to_string(),
        "llm_timeout" | "llm_upstream" | "llm_invalid_output" => {
            "종목을 찾지 못했습니다. 다시 시도하세요.".to_string()
        }
        "listings_unavailable" => {
            "공식 종목 목록을 아직 받지 못했습니다. 잠시 뒤에 다시 시도하세요.".to_string()
        }
        "already_registered" => "이미 등록한 종목입니다.".to_string(),
        "not_listed" => "공식 종목 목록에 없는 종목이라 등록할 수 없습니다.".to_string(),
        "not_found" => "이미 삭제된 종목입니다.".to_string(),
        "invalid" => "주제는 2~20자로 적으세요.".to_string(),
        "exists" => "이미 넣은 주제입니다.".to_string(),
        "unavailable" => {
            "worker가 토픽 갱신을 준비하지 못했습니다. worker를 다시 켜 보세요.".to_string()
        }
        "worker_timeout" => "worker 응답이 늦습니다. 다시 시도하세요.".to_string(),
        "worker_unreachable" => {
            "worker에 연결할 수 없습니다. worker가 켜져 있는지 확인하세요.".to_string()
        }
        _ => "처리하지 못했습니다. 다시 시도하세요.".to_string(),
    }
}

#[cfg(test)]
#[path = "messages_tests.rs"]
mod tests;
