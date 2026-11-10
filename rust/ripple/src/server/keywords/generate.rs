use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, params};

use crate::server::ai::decider::types::JudgeItem;
use crate::server::ai::llm::schemas::keyword_tree;
use crate::server::ai::llm::{LlmClient, LlmError, LlmErrorKind, StructuredRequest};
use crate::server::budget::usage::{
    BudgetExceededError, CallOptions, GenerateError, generate_with_budget,
};
use crate::server::db::client::Db;
use crate::server::db::schema::{ItemKind, KeywordStatus};
use crate::server::keywords::stat_words::{StatWords, stat_words};
use crate::server::keywords::tree::{
    KeywordNode, KeywordStock, build_keyword_messages, build_keyword_tree, clamp_depth,
    count_raw_keywords, parse_keyword_tree, refine_anchors,
};
use crate::server::log::{log, log_error};
use crate::server::settings::{KEYWORDS_DEPTH, LLM_MODEL, read_num, read_str};
use crate::server::stocks::aliases::validate_aliases;
use crate::server::stocks::repository::{Stock, find_stock, set_stock_terms};
use crate::server::text::js_slice;
use crate::server::time::{EpochMs, now_ms};

/// 2026-10-22 실측: 깊이 3에 14~16초(10단계). 11단계에서 후보를 단계마다 5~8개로 늘리자
/// 한국 증시 트리가 8,000토큰에 34초 걸려 잘렸다. 깊이 4와 느린 때를 생각해 넉넉히 둔다.
const KEYWORD_TIMEOUT_MS: u64 = 120_000;
/// 추론 토큰과 함께 쓴다(17단계 실측: 새 형식의 최대 출력은 2,142토큰)
const KEYWORD_MAX_TOKENS: u32 = 8_000;
/// 규칙에 안 맞는 답을 볼 때 로그에 남기는 LLM 원문 길이
const RAW_LOG_MAX_CHARS: usize = 6000;

/// 키워드 생성이 실패한 이유(화면에는 `describe_keyword_error` 문장이 보인다)
#[derive(Debug, thiserror::Error)]
enum KeywordError {
    #[error(transparent)]
    Budget(#[from] BudgetExceededError),
    /// LLM은 답했지만 규칙에 맞는 키워드가 하나도 없을 때
    #[error("규칙에 맞는 키워드가 나오지 않았습니다.")]
    NoUsableKeywords,
    /// 종목인데 고유어가 하나도 나오지 않았을 때(고유어가 비면 종목 키워드 연결이 모두 걸러진다)
    #[error("고유어가 나오지 않았습니다.")]
    NoAnchors,
    #[error(transparent)]
    Llm(#[from] LlmError),
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
}

impl From<GenerateError> for KeywordError {
    fn from(error: GenerateError) -> Self {
        match error {
            GenerateError::Budget(e) => Self::Budget(e),
            GenerateError::Llm(e) => Self::Llm(e),
            GenerateError::Db(e) => Self::Db(e),
        }
    }
}

/// 성공 결과
#[derive(Debug, Clone, PartialEq)]
pub struct GenerateOk {
    /// 저장한 키워드 수
    pub count: usize,
    /// 설정 깊이
    pub depth: i64,
    /// 실제로 만든 트리의 가장 깊은 단계
    pub deepest: i64,
    /// LLM이 줬지만 규칙(부모·중복·상한 등)에 맞지 않아 버린 키워드 수
    pub dropped: usize,
    /// 참고로 넣은 통계 낱말 수
    pub stat_words: usize,
    /// 저장한 별칭·고유어 수(종목이 아니면 0)
    pub aliases: usize,
    pub anchors: usize,
}

/// 실패 이유(error)는 화면에 "키워드를 만들지 못했습니다." 뒤에 붙여 보여 준다.
#[derive(Debug, Clone, PartialEq)]
pub enum GenerateResult {
    Ok(GenerateOk),
    Err(String),
}

/// 만들기 시작할 때 동기로 하는 일(종목 확인, 상태 '만드는 중', 깊이, 통계 낱말)의 결과.
/// 대기열이 첫 종목을 넣자마자 '만드는 중'으로 보이게 하려고 나눠 두었다(TS는 async 함수가 첫 await까지 바로 실행됨).
pub struct PreparedGeneration {
    stock_id: i64,
    stock: Stock,
    depth: i64,
    stat: StatWords,
}

pub struct GenerateOptions<'a> {
    pub now: Option<&'a (dyn Fn() -> EpochMs + Send + Sync)>,
}

impl GenerateOptions<'_> {
    fn now(&self) -> EpochMs {
        self.now.map_or_else(now_ms, |f| f())
    }
}

/// 항목(종목·시장·사용자 주제) 하나의 키워드 트리를 만든다(SPEC FR-3, 17단계).
/// LLM 한 번으로 깊이 2 트리를 상한 안에서 만들고, 종목은 별칭·고유어도 함께 받아 저장한다. Jev 평가는 하지 않는다.
/// 성공하면 이전 키워드를 통째로 바꾸되(D-16) 토픽 갱신이 붙인 가지는 남기고,
/// 실패하면 이전 키워드·별칭·고유어를 그대로 두고 이유를 남긴다. 자동 토픽 항목은 토픽 갱신이 검색어를 정하므로 만들지 않는다.
pub async fn generate_keywords_for_stock<L: LlmClient>(
    db: &Db,
    llm: &L,
    stock_id: i64,
    options: &GenerateOptions<'_>,
) -> GenerateResult {
    match prepare_generation(db, stock_id, options.now()) {
        Ok(prepared) => run_generation(db, llm, prepared, options).await,
        Err(result) => result,
    }
}

/// 종목 확인 → 상태 '만드는 중' → 깊이·통계 낱말. 만들 수 없으면 끝난 결과(`Err`)를 돌려준다.
pub fn prepare_generation(
    db: &Db,
    stock_id: i64,
    now: EpochMs,
) -> Result<PreparedGeneration, GenerateResult> {
    let conn = db.lock().expect("DB 잠금이 깨졌습니다");
    let db_error = |e: rusqlite::Error| GenerateResult::Err(e.to_string());
    let stock = match find_stock(&conn, stock_id).map_err(db_error)? {
        Some(stock) if stock.deleted_at.is_none() => stock,
        _ => {
            return Err(GenerateResult::Err(
                "종목이 없거나 삭제됐습니다.".to_string(),
            ));
        }
    };
    let topic_source: Option<String> = match stock.topic_id {
        Some(topic_id) => conn
            .query_row(
                "SELECT source FROM topics WHERE id = ?1",
                params![topic_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_error)?,
        None => None,
    };
    if topic_source.as_deref() == Some("auto") {
        return Err(GenerateResult::Err(
            "자동 토픽은 토픽 갱신이 검색어를 정합니다.".to_string(),
        ));
    }

    set_keyword_status(&conn, stock_id, KeywordStatus::Generating, None).map_err(db_error)?;
    let depth = clamp_depth(read_num(&conn, &KEYWORDS_DEPTH).map_err(db_error)?);
    let stat = if stock.kind == ItemKind::Stock {
        stat_words(&conn, stock_id, now).map_err(db_error)?
    } else {
        StatWords::default()
    };
    Ok(PreparedGeneration {
        stock_id,
        stock,
        depth,
        stat,
    })
}

/// `prepare_generation` 뒤의 나머지(LLM 호출 → 검증 → 저장). 실패하면 이유를 남기고 `Err`를 돌려준다.
pub async fn run_generation<L: LlmClient>(
    db: &Db,
    llm: &L,
    prepared: PreparedGeneration,
    options: &GenerateOptions<'_>,
) -> GenerateResult {
    let stock_id = prepared.stock_id;
    match try_generate(db, llm, &prepared, options).await {
        Ok(ok) => GenerateResult::Ok(ok),
        Err(error) => {
            // 화면에는 짧은 이유만 보이므로, 자세한 내용은 worker 로그에 남긴다.
            log_error(&format!("키워드 생성 실패: 종목 id {stock_id}"), &error);
            let message = describe_keyword_error(&error);
            {
                let conn = db.lock().expect("DB 잠금이 깨졌습니다");
                if let Err(e) =
                    set_keyword_status(&conn, stock_id, KeywordStatus::Failed, Some(&message))
                {
                    log_error(
                        &format!("키워드 실패 상태 기록 실패: 종목 id {stock_id}"),
                        &e,
                    );
                }
            }
            GenerateResult::Err(message)
        }
    }
}

async fn try_generate<L: LlmClient>(
    db: &Db,
    llm: &L,
    prepared: &PreparedGeneration,
    options: &GenerateOptions<'_>,
) -> Result<GenerateOk, KeywordError> {
    let stock_id = prepared.stock_id;
    let stock = &prepared.stock;
    let depth = prepared.depth;
    let target = KeywordStock {
        kind: stock.kind,
        market: stock.market,
        code: stock.code.clone(),
        name: stock.name.clone(),
        name_ko: stock.name_ko.clone(),
        exchange: stock.exchange.clone(),
    };
    let model = {
        let conn = db.lock().expect("DB 잠금이 깨졌습니다");
        read_str(&conn, &LLM_MODEL)?
    };
    let request = StructuredRequest {
        model,
        messages: build_keyword_messages(&target, depth, &prepared.stat),
        schema: keyword_tree(),
        schema_name: "keyword_tree".to_string(),
        max_tokens: KEYWORD_MAX_TOKENS,
        timeout_ms: Some(KEYWORD_TIMEOUT_MS),
        reasoning: crate::server::ai::llm::Reasoning::default(),
        parse: parse_keyword_tree,
    };
    let call = CallOptions {
        purpose: "keyword_generation",
        now: options.now,
        stock_id: Some(stock_id),
    };
    let value = generate_with_budget(db, llm, &request, &call).await?.value;

    let tree = build_keyword_tree(&target, &value, depth as f64);
    let dropped = count_raw_keywords(&value) - tree.len();
    let deepest = tree.iter().map(|n| n.level).max().unwrap_or(0);
    if dropped > 0 || deepest < depth {
        // 규칙에 맞지 않는 답(단계 누락 등)의 원인을 볼 수 있게 원문을 남긴다(10단계 결정).
        log(&format!(
            "키워드 LLM 원문(종목 id {stock_id}, 버림 {dropped}개, 깊이 {deepest}/{depth}): {}",
            js_slice(&value.to_json_string(), 0, RAW_LOG_MAX_CHARS)
        ));
    }
    if tree.is_empty() {
        return Err(KeywordError::NoUsableKeywords);
    }

    // 별칭·고유어는 종목만 저장한다. 고유어가 비면 종목의 키워드 연결이 모두 걸러지므로 실패로 둔다.
    let terms: Option<(Vec<String>, Vec<String>)> = if stock.kind == ItemKind::Stock {
        let anchors = refine_anchors(&value.anchors);
        if anchors.is_empty() {
            return Err(KeywordError::NoAnchors);
        }
        let aliases = {
            let conn = db.lock().expect("DB 잠금이 깨졌습니다");
            validate_aliases(&conn, stock, &value.aliases)?
        };
        Some((aliases, anchors))
    } else {
        None
    };
    let (alias_count, anchor_count) = terms
        .as_ref()
        .map_or((0, 0), |(aliases, anchors)| (aliases.len(), anchors.len()));
    {
        let mut conn = db.lock().expect("DB 잠금이 깨졌습니다");
        replace_tree(
            &mut conn,
            stock_id,
            &tree,
            terms.as_ref().map(|(a, b)| (a.as_slice(), b.as_slice())),
            options.now(),
        )?;
    }
    Ok(GenerateOk {
        count: tree.len(),
        depth,
        deepest,
        dropped,
        stat_words: prepared.stat.words.len(),
        aliases: alias_count,
        anchors: anchor_count,
    })
}

/// Jev 판단 대상(종목·시장·주제)
pub fn judge_item(stock: &Stock, topic_summary: Option<&str>) -> JudgeItem {
    JudgeItem {
        kind: stock.kind.into(),
        name: stock.name_ko.clone().unwrap_or_else(|| stock.name.clone()),
        official_name: stock
            .name_ko
            .as_ref()
            .filter(|ko| !ko.is_empty() && **ko != stock.name)
            .map(|_| stock.name.clone()),
        market: stock.market,
        exchange: stock.exchange.clone(),
        code: stock.code.clone(),
        summary: topic_summary.map(str::to_string),
    }
}

/// 한 트랜잭션으로 이전 키워드를 지우고 새 트리를 넣는다(읽는 쪽은 이전 또는 새 트리만 본다).
/// 토픽 갱신이 붙인 가지(topic_id가 있는 키워드)는 남긴다(11단계).
pub fn replace_keywords(
    conn: &mut Connection,
    stock_id: i64,
    tree: &[KeywordNode],
    at: EpochMs,
) -> rusqlite::Result<()> {
    replace_tree(conn, stock_id, tree, None, at)
}

/// `replace_keywords`와 같되, `terms`(별칭, 고유어)가 있으면 같은 트랜잭션에서 함께 바꾼다(17단계).
pub fn replace_tree(
    conn: &mut Connection,
    stock_id: i64,
    tree: &[KeywordNode],
    terms: Option<(&[String], &[String])>,
    at: EpochMs,
) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    if let Some((aliases, anchors)) = terms {
        set_stock_terms(&tx, stock_id, aliases, anchors)?;
    }
    tx.execute(
        "DELETE FROM keywords WHERE stock_id = ?1 AND topic_id IS NULL",
        params![stock_id],
    )?;
    let mut ids: HashMap<i64, i64> = HashMap::new();
    {
        let mut insert = tx.prepare(
            "INSERT INTO keywords (stock_id, level, parent_id, text_ko, text_en, reason, path, kind, relevance, magnitude, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )?;
        for node in tree {
            let parent_id = node.parent_key.and_then(|key| ids.get(&key).copied());
            insert.execute(params![
                stock_id,
                node.level,
                parent_id,
                node.text_ko,
                node.text_en,
                node.reason,
                node.path,
                node.kind.as_str(),
                node.relevance,
                node.magnitude,
                at
            ])?;
            ids.insert(node.key, tx.last_insert_rowid());
        }
    }
    tx.execute(
        "UPDATE stocks SET keywords_status = 'ready', keywords_error = NULL, keywords_generated_at = ?1 WHERE id = ?2",
        params![at, stock_id],
    )?;
    tx.commit()
}

pub fn set_keyword_status(
    conn: &Connection,
    stock_id: i64,
    status: KeywordStatus,
    error: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE stocks SET keywords_status = ?1, keywords_error = ?2 WHERE id = ?3",
        params![status.as_str(), error, stock_id],
    )?;
    Ok(())
}

/// worker가 시작할 때 이어서 만들 종목(대기 중이었거나 만들다 멈춘 것)
pub fn stocks_awaiting_keywords(conn: &Connection) -> rusqlite::Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM stocks WHERE deleted_at IS NULL AND keywords_status IN ('pending', 'generating') ORDER BY id ASC",
    )?;
    stmt.query_map([], |row| row.get(0))?.collect()
}

fn describe_keyword_error(error: &KeywordError) -> String {
    match error {
        KeywordError::Budget(_) => "이번 달 AI 예산을 모두 썼습니다.",
        KeywordError::NoUsableKeywords => "규칙에 맞는 키워드가 나오지 않았습니다.",
        KeywordError::NoAnchors => "고유어가 나오지 않았습니다.",
        KeywordError::Llm(e) => match e.kind {
            LlmErrorKind::NoApiKey => "OpenRouter API 키가 설정되지 않았습니다.",
            LlmErrorKind::PaymentRequired => {
                "OpenRouter 크레딧이 부족하거나 키의 사용 한도에 걸렸습니다."
            }
            LlmErrorKind::RateLimited => "요청이 많아 잠시 막혔습니다.",
            LlmErrorKind::Timeout => "AI 응답이 늦어 중단했습니다.",
            LlmErrorKind::InvalidOutput => "AI 응답 형식이 올바르지 않았습니다.",
            LlmErrorKind::Upstream => "AI 서비스에 연결하지 못했거나 오류가 났습니다.",
        },
        KeywordError::Db(_) => "알 수 없는 오류가 났습니다.",
    }
    .to_string()
}

#[cfg(test)]
#[path = "generate_tests.rs"]
mod tests;
