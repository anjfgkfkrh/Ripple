use std::collections::HashSet;

use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;

use crate::server::ai::llm::schemas::stock_candidates;
use crate::server::ai::llm::{LlmClient, LlmError, LlmMessage, StructuredRequest};
use crate::server::budget::usage::{
    BudgetExceededError, CallOptions, GenerateError, generate_with_budget,
};
use crate::server::db::client::Db;
use crate::server::db::schema::Market;
use crate::server::listings::lookup::{find_listing, find_listing_by_name};
use crate::server::listings::sync::listing_count;
use crate::server::settings::{LLM_MODEL, read_str};
use crate::server::stocks::repository::{MAX_NAME_KO_LENGTH, is_active_stock};
use crate::server::text::{js_slice, js_trim};
use crate::server::time::EpochMs;

/// 후보는 최대 3개
pub const MAX_CANDIDATES: usize = 3;
pub const MAX_QUERY_LENGTH: usize = 50;

/// LLM 출력 후보 하나(`matchOutputSchema`). 같은 스키마(`schemas::stock_candidates`)로 구조화 출력을 요청하고 응답을 검증한다.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmCandidate {
    pub market: Market,
    pub code: String,
    pub name: String,
    pub name_ko: String,
}

#[derive(Deserialize)]
struct MatchOutput {
    candidates: Vec<LlmCandidate>,
}

fn parse_output(value: &Value) -> Option<Vec<LlmCandidate>> {
    serde_json::from_value::<MatchOutput>(value.clone())
        .ok()
        .map(|o| o.candidates)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockCandidate {
    pub market: Market,
    pub code: String,
    pub name: String,
    pub name_ko: Option<String>,
    pub exchange: String,
    pub is_etf: bool,
    pub already_registered: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("공식 종목 목록을 아직 받지 못했습니다.")]
pub struct ListingsUnavailableError;

#[derive(Debug, thiserror::Error)]
pub enum MatchError {
    #[error(transparent)]
    Budget(#[from] BudgetExceededError),
    #[error(transparent)]
    Listings(#[from] ListingsUnavailableError),
    #[error(transparent)]
    Llm(#[from] LlmError),
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
}

impl From<GenerateError> for MatchError {
    fn from(error: GenerateError) -> Self {
        match error {
            GenerateError::Budget(e) => Self::Budget(e),
            GenerateError::Llm(e) => Self::Llm(e),
            GenerateError::Db(e) => Self::Db(e),
        }
    }
}

pub fn build_match_messages(query: &str) -> Vec<LlmMessage> {
    vec![
        LlmMessage::system(
            [
                "너는 한국·미국 상장 주식의 종목을 찾아 주는 도우미다.",
                "사용자가 적은 종목명·줄임말·별명·영문명·티커·종목코드를 실제 상장 종목으로 바꿔, 가능성이 높은 순서로 최대 3개 후보를 낸다.",
                "- market: 한국 거래소(KOSPI·KOSDAQ·KONEX) 상장이면 KR, 미국 거래소(NASDAQ·NYSE 등) 상장이면 US.",
                "- code: KR은 6자리 종목코드, US는 티커.",
                "- name: 정식 회사 이름. KR은 한글, US는 영문.",
                "- nameKo: 한국에서 흔히 부르는 한글 이름(예: NVIDIA → 엔비디아).",
                "- 확실한 종목이 하나면 하나만 낸다. 어떤 종목인지 모르겠으면 빈 목록을 낸다. 없는 종목을 지어내지 않는다.",
            ]
            .join("\n"),
        ),
        LlmMessage::user(query),
    ]
}

/// LLM 후보를 공식 목록과 대조한다(D-12). 목록에 없는 후보는 버리고 이름·거래소는 목록 값을 쓴다.
/// 한국 종목은 코드가 틀려도 정식 이름이 목록과 같으면 목록의 코드를 쓴다.
pub fn verify_candidates(
    conn: &Connection,
    candidates: &[LlmCandidate],
) -> rusqlite::Result<Vec<StockCandidate>> {
    let mut verified = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for candidate in candidates {
        let mut listing = find_listing(conn, candidate.market, &candidate.code)?;
        if listing.is_none() && candidate.market == Market::Kr {
            listing = find_listing_by_name(conn, Market::Kr, &candidate.name)?;
        }
        let Some(listing) = listing else { continue };
        let key = format!("{}:{}", listing.market.as_str(), listing.code);
        if !seen.insert(key) {
            continue;
        }
        let name_ko = if listing.market == Market::Kr {
            Some(listing.name.clone())
        } else {
            Some(js_slice(js_trim(&candidate.name_ko), 0, MAX_NAME_KO_LENGTH))
                .filter(|n| !n.is_empty())
        };
        verified.push(StockCandidate {
            market: listing.market,
            already_registered: is_active_stock(conn, listing.market, &listing.code)?,
            code: listing.code,
            name: listing.name,
            name_ko,
            exchange: listing.exchange,
            is_etf: listing.is_etf,
        });
        if verified.len() == MAX_CANDIDATES {
            break;
        }
    }
    Ok(verified)
}

/// 자유 입력 → 후보(최대 3개). AI 호출은 예산 검사·사용량 기록을 거친다.
pub async fn match_stocks(
    db: &Db,
    llm: &impl LlmClient,
    query: &str,
    now: Option<&(dyn Fn() -> EpochMs + Send + Sync)>,
) -> Result<Vec<StockCandidate>, MatchError> {
    let model = {
        let conn = db.lock().expect("DB 잠금이 깨졌습니다");
        if listing_count(&conn, Market::Kr)? == 0 && listing_count(&conn, Market::Us)? == 0 {
            return Err(ListingsUnavailableError.into());
        }
        read_str(&conn, &LLM_MODEL)?
    };
    let request = StructuredRequest {
        model,
        messages: build_match_messages(js_trim(query)),
        schema: stock_candidates(),
        schema_name: "stock_candidates".to_string(),
        // 추론(생각) 토큰도 이 한도를 함께 쓴다(Claude 5.5 계열). 최대 비용은 1회 $0.001 수준.
        max_tokens: 2000,
        timeout_ms: None,
        parse: parse_output,
    };
    let options = CallOptions {
        purpose: "stock_match",
        now,
        stock_id: None,
    };
    let result = generate_with_budget(db, llm, &request, &options).await?;
    let conn = db.lock().expect("DB 잠금이 깨졌습니다");
    Ok(verify_candidates(&conn, &result.value)?)
}

#[cfg(test)]
#[path = "match_tests.rs"]
mod tests;
