use rusqlite::{Connection, params};

use crate::server::ai::decider::types::{Decider, DeciderError, LeadInput, LeadRating};
use crate::server::ai::llm::{
    LlmClient, LlmError, LlmUsage, StructuredRequest, StructuredResult, generate_object,
};
use crate::server::db::client::Db;
use crate::server::db::schema::UsageProvider;
use crate::server::settings::{
    BUDGET_MONTHLY_USD, JEV_MODEL, JEV_USD_PER_MILLION_INPUT_TOKENS, read_num, read_str,
};
use crate::server::text::{js_number, js_to_fixed};
use crate::server::time::{EpochMs, now_ms, start_of_seoul_month};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("이번 달 AI 예산을 모두 썼습니다(사용 ${spent} / 한도 ${limit}).", spent = js_to_fixed(*.spent_usd, 4), limit = js_number(*.limit_usd))]
pub struct BudgetExceededError {
    pub spent_usd: f64,
    pub limit_usd: f64,
}

/// 이번 달(한국 시간) AI 사용액 합계, 달러
pub fn month_spend_usd(conn: &Connection, now: EpochMs) -> rusqlite::Result<f64> {
    conn.query_row(
        "SELECT coalesce(sum(cost_usd), 0) FROM usage_logs WHERE at >= ?1",
        params![start_of_seoul_month(now)],
        |row| row.get(0),
    )
}

pub struct UsageEntry<'a> {
    pub provider: UsageProvider,
    pub model: &'a str,
    pub purpose: &'a str,
    pub usage: LlmUsage,
    pub at: EpochMs,
    /// 어느 항목을 위한 호출인지(항목별 비용, 11단계). 여러 항목 공용이면 None
    pub stock_id: Option<i64>,
}

pub fn record_usage(conn: &Connection, entry: &UsageEntry<'_>) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO usage_logs (at, provider, model, purpose, input_tokens, output_tokens, cost_usd, stock_id) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            entry.at,
            entry.provider.as_str(),
            entry.model,
            entry.purpose,
            entry.usage.input_tokens,
            entry.usage.output_tokens,
            entry.usage.cost_usd,
            entry.stock_id,
        ],
    )?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum BudgetCheckError {
    #[error(transparent)]
    Exceeded(#[from] BudgetExceededError),
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
}

/// 이번 달 사용액이 한도에 닿았으면 BudgetExceededError. AI 호출 전에 부른다(CLAUDE.md).
pub fn assert_within_budget(conn: &Connection, now: EpochMs) -> Result<(), BudgetCheckError> {
    let limit_usd = read_num(conn, &BUDGET_MONTHLY_USD)?;
    let spent_usd = month_spend_usd(conn, now)?;
    if spent_usd >= limit_usd {
        return Err(BudgetExceededError {
            spent_usd,
            limit_usd,
        }
        .into());
    }
    Ok(())
}

/// AI 호출 하나에 붙는 정보. `now`가 없으면 지금 시각(호출 앞뒤 두 번 읽음).
pub struct CallOptions<'a> {
    pub purpose: &'a str,
    pub now: Option<&'a (dyn Fn() -> EpochMs + Send + Sync)>,
    pub stock_id: Option<i64>,
}

impl<'a> CallOptions<'a> {
    pub fn new(purpose: &'a str) -> Self {
        Self {
            purpose,
            now: None,
            stock_id: None,
        }
    }

    fn now(&self) -> EpochMs {
        self.now.map_or_else(now_ms, |f| f())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GenerateError {
    #[error(transparent)]
    Budget(#[from] BudgetExceededError),
    #[error(transparent)]
    Llm(#[from] LlmError),
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
}

impl From<BudgetCheckError> for GenerateError {
    fn from(error: BudgetCheckError) -> Self {
        match error {
            BudgetCheckError::Exceeded(e) => Self::Budget(e),
            BudgetCheckError::Db(e) => Self::Db(e),
        }
    }
}

/// AI 호출은 반드시 이 함수를 거친다(CLAUDE.md): 월 한도 검사 → 호출 → 사용량 기록.
/// 응답을 받은 뒤 형식이 틀려 실패해도 청구된 비용은 기록한다.
/// DB 잠금은 검사·기록 때만 짧게 잡고 호출을 기다리는 동안은 쥐지 않는다.
pub async fn generate_with_budget<T: Send>(
    db: &Db,
    llm: &impl LlmClient,
    request: &StructuredRequest<T>,
    options: &CallOptions<'_>,
) -> Result<StructuredResult<T>, GenerateError> {
    {
        let conn = db.lock().expect("DB 잠금이 깨졌습니다");
        assert_within_budget(&conn, options.now())?;
    }

    let record = |model: &str, usage: LlmUsage| -> rusqlite::Result<()> {
        let conn = db.lock().expect("DB 잠금이 깨졌습니다");
        record_usage(
            &conn,
            &UsageEntry {
                provider: llm.provider(),
                model,
                purpose: options.purpose,
                usage,
                at: options.now(),
                stock_id: options.stock_id,
            },
        )
    };
    match generate_object(llm, request).await {
        Ok(result) => {
            record(&result.model, result.usage)?;
            Ok(result)
        }
        Err(error) => {
            if let Some(usage) = error.usage {
                record(&request.model, usage)?;
            }
            Err(error.into())
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RateError {
    #[error(transparent)]
    Budget(#[from] BudgetExceededError),
    #[error(transparent)]
    Decider(#[from] DeciderError),
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
}

impl From<BudgetCheckError> for RateError {
    fn from(error: BudgetCheckError) -> Self {
        match error {
            BudgetCheckError::Exceeded(e) => Self::Budget(e),
            BudgetCheckError::Db(e) => Self::Db(e),
        }
    }
}

/// Jev로 키워드·토픽을 평가한다(11단계). AI 호출 원칙대로 월 한도 검사 → 호출 → 사용량 기록(입력 토큰 × 단가, D-06).
pub async fn rate_with_budget(
    db: &Db,
    decider: &impl Decider,
    input: &LeadInput,
    options: &CallOptions<'_>,
) -> Result<LeadRating, RateError> {
    let model = {
        let conn = db.lock().expect("DB 잠금이 깨졌습니다");
        assert_within_budget(&conn, options.now())?;
        read_str(&conn, &JEV_MODEL)?
    };
    let rating = decider.rate(input, &model).await?;
    let conn = db.lock().expect("DB 잠금이 깨졌습니다");
    let cost_usd = (rating.usage.input_tokens as f64
        * read_num(&conn, &JEV_USD_PER_MILLION_INPUT_TOKENS)?)
        / 1_000_000.0;
    record_usage(
        &conn,
        &UsageEntry {
            provider: UsageProvider::Typesafe,
            model: &rating.model,
            purpose: options.purpose,
            usage: LlmUsage {
                input_tokens: rating.usage.input_tokens,
                output_tokens: rating.usage.output_tokens,
                cost_usd,
            },
            at: options.now(),
            stock_id: options.stock_id,
        },
    )?;
    Ok(rating)
}

#[cfg(test)]
#[path = "usage_tests.rs"]
mod tests;
