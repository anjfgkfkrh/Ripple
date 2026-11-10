use serde_json::json;

use super::*;
use crate::server::ai::decider::types::DeciderErrorKind;
use crate::server::ai::llm::{JsonResult, LlmClient, LlmUsage, StructuredRequest};
use crate::server::budget::usage::{UsageEntry, record_usage};
use crate::server::db::schema::{Market, UsageProvider};
use crate::server::keywords::read::list_keywords_by_stock;
use crate::server::settings::{KEYWORDS_DEPTH, KEYWORDS_MIN_RELEVANCE, write_num};
use crate::server::stocks::repository::{
    RegisterInput, RegisterResult, delete_stock, register_stock,
};
use crate::test_support::{
    LinkSpec, SharedTestDb, attach_topic, fake_decider, fake_llm, fake_rating, insert_link, ms,
    sample_keyword_tree, seed_lens, seed_listings, seed_market, seed_topic, shared_test_db,
};

fn now() -> EpochMs {
    ms("2026-10-20T03:00:00Z")
}

fn later() -> EpochMs {
    ms("2026-10-21T03:00:00Z")
}

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

fn register(shared: &SharedTestDb, market: Market) -> i64 {
    let conn = shared.db.lock().unwrap();
    let input = match market {
        Market::Kr => RegisterInput {
            market,
            code: "005930",
            user_input: "삼전",
            name_ko: None,
        },
        Market::Us => RegisterInput {
            market,
            code: "NVDA",
            user_input: "NVDA",
            name_ko: Some("엔비디아"),
        },
    };
    match register_stock(&conn, &input, now()).unwrap() {
        RegisterResult::Created(stock) => stock.id,
        _ => panic!("표본에 없는 종목"),
    }
}

struct Row {
    id: i64,
    level: i64,
    parent_id: Option<i64>,
    text_ko: String,
    text_en: Option<String>,
    reason: String,
    path: String,
    kind: String,
    relevance: Option<f64>,
    magnitude: Option<f64>,
    topic_id: Option<i64>,
    created_at: i64,
}

fn keyword_rows(shared: &SharedTestDb, stock_id: i64) -> Vec<Row> {
    let conn = shared.db.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, level, parent_id, text_ko, text_en, reason, path, kind, relevance, magnitude, topic_id, created_at \
             FROM keywords WHERE stock_id = ?1 ORDER BY id ASC",
        )
        .unwrap();
    stmt.query_map([stock_id], |r| {
        Ok(Row {
            id: r.get(0)?,
            level: r.get(1)?,
            parent_id: r.get(2)?,
            text_ko: r.get(3)?,
            text_en: r.get(4)?,
            reason: r.get(5)?,
            path: r.get(6)?,
            kind: r.get(7)?,
            relevance: r.get(8)?,
            magnitude: r.get(9)?,
            topic_id: r.get(10)?,
            created_at: r.get(11)?,
        })
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

fn stock_state(shared: &SharedTestDb, id: i64) -> (String, Option<String>, Option<i64>) {
    shared
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT keywords_status, keywords_error, keywords_generated_at FROM stocks WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap()
}

fn usage_purposes(shared: &SharedTestDb) -> Vec<(String, String, Option<i64>)> {
    let conn = shared.db.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT purpose, model, stock_id FROM usage_logs ORDER BY id")
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// 늘 같은 종류의 LLM 오류를 내는 가짜(`failingLlm`)
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

fn tree_llm() -> crate::test_support::FakeLlm {
    fake_llm(|_| sample_keyword_tree(false), USAGE)
}

fn opts<'a, D>(
    at: Option<&'a (dyn Fn() -> EpochMs + Send + Sync)>,
    decider: Option<&'a D>,
) -> GenerateOptions<'a, D> {
    GenerateOptions { now: at, decider }
}

async fn generate_plain(
    shared: &SharedTestDb,
    llm: &impl LlmClient,
    id: i64,
    at: Option<&(dyn Fn() -> EpochMs + Send + Sync)>,
) -> GenerateResult {
    generate_keywords_for_stock(&shared.db, llm, id, &opts::<NoDecider>(at, None)).await
}

fn ok(result: GenerateResult) -> GenerateOk {
    match result {
        GenerateResult::Ok(ok) => ok,
        GenerateResult::Err(e) => panic!("실패: {e}"),
    }
}

// describe("generateKeywordsForStock (SPEC FR-3)")
#[tokio::test(flavor = "current_thread")]
async fn one_llm_call_builds_tree_saves_it_and_marks_ready() {
    // it("LLM 1번으로 트리를 만들어 저장하고 상태를 완료로 바꾼다")
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    let llm = tree_llm();
    let at = || now();

    let result = generate_plain(&shared, &llm, id, Some(&at)).await;

    // Jev가 없으면 후보를 자르지 않고 상한까지 남긴다(11단계).
    assert_eq!(
        result,
        GenerateResult::Ok(GenerateOk {
            count: 10,
            depth: 2,
            deepest: 2,
            dropped: 0,
            candidates: 10,
            rated: 0,
            unrated_reason: Some("TYPESAFE_API_KEY가 없습니다.".into()),
            headlines: 0,
        })
    );
    assert_eq!(llm.call_count(), 1);
    {
        let calls = llm.calls.lock().unwrap();
        assert_eq!(calls[0].model, "anthropic/claude-haiku-5.5");
        assert_eq!(calls[0].schema_name, "keyword_tree");
        assert_eq!(calls[0].max_tokens, 16_000);
    }
    assert_eq!(
        stock_state(&shared, id),
        ("ready".into(), None, Some(now()))
    );

    let rows = keyword_rows(&shared, id);
    assert_eq!(rows.len(), 10);
    let hbm = rows.iter().find(|r| r.text_ko == "HBM").unwrap();
    let nvidia = rows.iter().find(|r| r.text_ko == "엔비디아").unwrap();
    assert_eq!(nvidia.level, 2);
    assert_eq!(nvidia.parent_id, Some(hbm.id));
    assert_eq!(nvidia.text_en, None);
    assert_eq!(nvidia.reason, "HBM 주요 고객");
    assert_eq!(nvidia.path, "삼성전자 → HBM → 엔비디아");
    assert_eq!(nvidia.created_at, now());

    // 호출마다 사용량을 남긴다(비용 장치).
    assert_eq!(
        usage_purposes(&shared),
        [(
            "keyword_generation".to_string(),
            "anthropic/claude-haiku-5.5".to_string(),
            Some(id)
        )]
    );
}

#[test]
fn request_timeout_is_120_seconds() {
    // it(...)의 `timeoutMs: 120_000` 확인: 요청에 담는 값은 상수와 같다.
    assert_eq!(KEYWORD_TIMEOUT_MS, 120_000);
}

#[tokio::test(flavor = "current_thread")]
async fn recent_headlines_of_the_stock_are_sent_and_counted() {
    // it("그 종목의 최근 주요 기사 제목을 함께 넣고, 넣은 수를 알려 준다")
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    insert_link(
        &shared.db.lock().unwrap(),
        &LinkSpec::new(id, "삼성전자 노조 협상", now()).with_relevance(8.0),
    );
    let llm = tree_llm();
    let at = || later();

    let result = ok(generate_plain(&shared, &llm, id, Some(&at)).await);

    assert_eq!(result.headlines, 1);
    assert!(
        llm.calls.lock().unwrap()[0].messages[1]
            .content
            .contains("- 삼성전자 노조 협상")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn feed_titles_and_topics_do_not_change_the_tree_or_add_market_issue_calls() {
    // it("피드 제목·토픽이 있어도 시장 이슈를 정리하거나 트리에 넣지 않고, 구조 키워드만 만든다(2026-10-23)")
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    {
        let conn = shared.db.lock().unwrap();
        for i in 0..60 {
            conn.execute(
                "INSERT INTO feed_headlines (title_key, title, publisher, feed, published_at, seen_at) \
                 VALUES (?1, ?2, '테스트', 'rss:test:economy', ?3, ?4)",
                rusqlite::params![format!("k{i}"), format!("경제 기사 {}", i + 1), later() - i * 60_000, later()],
            )
            .unwrap();
        }
        seed_topic(&conn, "auto", "중동 긴장", &["사우디"], later());
    }
    let llm = tree_llm();
    let at = || later();
    let result = ok(generate_plain(&shared, &llm, id, Some(&at)).await);
    assert_eq!(result.count, 10);
    let calls = llm.calls.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .map(|c| c.schema_name.as_str())
            .collect::<Vec<_>>(),
        ["keyword_tree"]
    );
    assert!(!calls[0].messages[1].content.contains("중동 긴장"));
    assert!(!calls[0].messages[1].content.contains("시장 이슈"));
    drop(calls);
    assert!(
        keyword_rows(&shared, id)
            .iter()
            .all(|r| r.kind == "structure")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn raw_llm_answer_is_logged_when_rules_drop_keywords_or_levels_are_missing() {
    // it("규칙에 안 맞아 버리거나 단계가 모자라면 LLM 원문을 로그에 남긴다")
    // 로그는 표준 출력으로 나가므로, 같은 조건의 값(버림·깊이)은 결과로 확인하고 문구 모양은 따로 확인한다.
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    // 깊이 2인데 1단계만 준다.
    let level1_only = fake_llm(
        |_| {
            let mut tree = sample_keyword_tree(false);
            tree["keywords"]
                .as_array_mut()
                .unwrap()
                .retain(|k| k["level"] == 1);
            tree
        },
        USAGE,
    );
    let result = ok(generate_plain(&shared, &level1_only, id, None).await);
    assert_eq!((result.dropped, result.candidates), (0, 5));
    // 깊이 1/2: 원문 기록 조건(`deepest < depth`)이 참이다.
    let raw = parse_keyword_tree(
        &json!({ "keywords": sample_keyword_tree(false)["keywords"].as_array().unwrap()[..2] }),
    )
    .unwrap();
    let line = format!(
        "키워드 LLM 원문(종목 id {id}, 버림 0개, 깊이 1/2): {}",
        js_slice(&raw.to_json_string(), 0, RAW_LOG_MAX_CHARS)
    );
    assert!(line.starts_with("키워드 LLM 원문(종목 id 1, 버림 0개, 깊이 1/2): {\"keywords\":[{\"level\":1,\"keyword\":\"메모리 반도체\""));
}

#[tokio::test(flavor = "current_thread")]
async fn status_is_generating_while_the_llm_is_called() {
    // it("만드는 동안에는 상태가 '만드는 중'이다")
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    let seen = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
    let (db, seen_in) = (shared.db.clone(), seen.clone());
    let llm = fake_llm(
        move |_| {
            let status: String = db
                .lock()
                .unwrap()
                .query_row(
                    "SELECT keywords_status FROM stocks WHERE id = ?1",
                    [id],
                    |r| r.get(0),
                )
                .unwrap();
            *seen_in.lock().unwrap() = Some(status);
            sample_keyword_tree(false)
        },
        USAGE,
    );
    generate_plain(&shared, &llm, id, None).await;
    assert_eq!(seen.lock().unwrap().as_deref(), Some("generating"));
}

#[tokio::test(flavor = "current_thread")]
async fn us_stock_saves_english_too() {
    // it("미국 종목은 영어 표기를 함께 저장한다(D-14)")
    let shared = seeded();
    let id = register(&shared, Market::Us);
    let llm = fake_llm(|_| sample_keyword_tree(true), USAGE);
    generate_plain(&shared, &llm, id, None).await;
    let rows = keyword_rows(&shared, id);
    let hbm = rows.iter().find(|r| r.text_ko == "HBM").unwrap();
    assert_eq!(hbm.text_en.as_deref(), Some("HBM"));
    assert_eq!(hbm.path, "엔비디아 → HBM");
}

#[tokio::test(flavor = "current_thread")]
async fn depth_follows_setting_and_is_clamped_to_1_to_4() {
    // it("깊이는 설정 값을 따르고 1~4로 맞춘다")
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    let llm = tree_llm();

    write_num(&shared.db.lock().unwrap(), &KEYWORDS_DEPTH, 1.0, now()).unwrap();
    // 깊이 1이면 2단계 5개는 버린다.
    let result = ok(generate_plain(&shared, &llm, id, None).await);
    assert_eq!(
        (
            result.count,
            result.depth,
            result.dropped,
            result.candidates
        ),
        (5, 1, 5, 5)
    );
    assert!(
        llm.calls.lock().unwrap()[0]
            .messages
            .last()
            .unwrap()
            .content
            .contains("깊이: 1")
    );

    write_num(&shared.db.lock().unwrap(), &KEYWORDS_DEPTH, 9.0, now()).unwrap();
    generate_plain(&shared, &llm, id, None).await;
    assert!(
        llm.calls.lock().unwrap()[1]
            .messages
            .last()
            .unwrap()
            .content
            .contains("깊이: 4")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn regenerating_replaces_previous_keywords_whole() {
    // it("다시 만들면 이전 키워드를 통째로 바꾼다(D-15·D-16)")
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    let (first, second) = (|| now(), || later());
    generate_plain(&shared, &tree_llm(), id, Some(&first)).await;
    let replacement = fake_llm(
        |_| json!({ "keywords": [{ "level": 1, "keyword": "반도체", "keywordEn": "", "parent": "", "reason": "r" }] }),
        USAGE,
    );
    generate_plain(&shared, &replacement, id, Some(&second)).await;

    assert_eq!(
        keyword_rows(&shared, id)
            .iter()
            .map(|r| r.path.as_str())
            .collect::<Vec<_>>(),
        ["삼성전자 → 반도체"]
    );
    assert_eq!(stock_state(&shared, id).2, Some(later()));
}

#[tokio::test(flavor = "current_thread")]
async fn failure_keeps_reason_and_previous_keywords() {
    // it("실패하면 이유를 남기고 이전 키워드는 그대로 둔다")
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    let (first, second) = (|| now(), || later());
    generate_plain(&shared, &tree_llm(), id, Some(&first)).await;

    let result = generate_plain(
        &shared,
        &FailingLlm(LlmErrorKind::Upstream),
        id,
        Some(&second),
    )
    .await;

    let message = "AI 서비스에 연결하지 못했거나 오류가 났습니다.";
    assert_eq!(result, GenerateResult::Err(message.into()));
    assert_eq!(
        stock_state(&shared, id),
        ("failed".into(), Some(message.into()), Some(now()))
    );
    assert_eq!(keyword_rows(&shared, id).len(), 10);
}

#[tokio::test(flavor = "current_thread")]
async fn llm_error_kinds_become_screen_reasons() {
    // it.each([...])("LLM 오류 %s → 화면에 보일 이유")
    for (kind, message) in [
        (
            LlmErrorKind::NoApiKey,
            "OpenRouter API 키가 설정되지 않았습니다.",
        ),
        (
            LlmErrorKind::PaymentRequired,
            "OpenRouter 크레딧이 부족하거나 키의 사용 한도에 걸렸습니다.",
        ),
        (LlmErrorKind::RateLimited, "요청이 많아 잠시 막혔습니다."),
        (LlmErrorKind::Timeout, "AI 응답이 늦어 중단했습니다."),
        (
            LlmErrorKind::InvalidOutput,
            "AI 응답 형식이 올바르지 않았습니다.",
        ),
    ] {
        let shared = seeded();
        let id = register(&shared, Market::Kr);
        let result = generate_plain(&shared, &FailingLlm(kind), id, None).await;
        assert_eq!(result, GenerateResult::Err(message.into()), "{kind:?}");
        assert_eq!(stock_state(&shared, id).1.as_deref(), Some(message));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn unexpected_non_llm_error_is_recorded_as_failure() {
    // it("LLM 오류가 아닌 예기치 못한 오류도 실패로 남긴다")
    // Rust의 LLM 클라이언트는 `LlmError`만 낸다. LLM 밖에서 나는 예기치 못한 오류는 DB 오류이므로 그것으로 확인한다.
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    shared
        .db
        .lock()
        .unwrap()
        .execute_batch("DROP TABLE keywords")
        .unwrap();
    let result = generate_plain(&shared, &tree_llm(), id, None).await;
    assert_eq!(
        result,
        GenerateResult::Err("알 수 없는 오류가 났습니다.".into())
    );
    assert_eq!(stock_state(&shared, id).0, "failed");
}

#[tokio::test(flavor = "current_thread")]
async fn exhausted_budget_blocks_the_llm_and_records_failure() {
    // it("예산을 다 썼으면 LLM을 부르지 않고 실패로 남긴다")
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    record_usage(
        &shared.db.lock().unwrap(),
        &UsageEntry {
            provider: UsageProvider::Openrouter,
            model: "m",
            purpose: "t",
            usage: LlmUsage {
                input_tokens: 1,
                output_tokens: 1,
                cost_usd: 5.0,
            },
            at: crate::server::time::now_ms(),
            stock_id: None,
        },
    )
    .unwrap();
    let llm = tree_llm();

    let result = generate_plain(&shared, &llm, id, None).await;
    assert_eq!(
        result,
        GenerateResult::Err("이번 달 AI 예산을 모두 썼습니다.".into())
    );
    assert_eq!(llm.call_count(), 0);
    assert_eq!(stock_state(&shared, id).0, "failed");
}

#[tokio::test(flavor = "current_thread")]
async fn no_usable_keywords_is_failure() {
    // it("규칙에 맞는 키워드가 하나도 없으면 실패")
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    let llm = fake_llm(
        |_| json!({ "keywords": [{ "level": 1, "keyword": "삼성전자", "keywordEn": "", "parent": "", "reason": "" }] }),
        USAGE,
    );
    let result = generate_plain(&shared, &llm, id, None).await;
    assert_eq!(
        result,
        GenerateResult::Err("규칙에 맞는 키워드가 나오지 않았습니다.".into())
    );
    assert!(keyword_rows(&shared, id).is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn missing_or_deleted_stock_does_not_call_llm() {
    // it("없거나 삭제한 종목은 LLM을 부르지 않는다")
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    delete_stock(&shared.db.lock().unwrap(), id, now()).unwrap();
    let llm = tree_llm();

    assert!(matches!(
        generate_plain(&shared, &llm, id, None).await,
        GenerateResult::Err(_)
    ));
    assert!(matches!(
        generate_plain(&shared, &llm, id + 100, None).await,
        GenerateResult::Err(_)
    ));
    assert_eq!(llm.call_count(), 0);
}

// describe("generateKeywordsForStock: Jev 점수로 고르기 (11단계)")
fn rating_with(
    relevance: Option<f64>,
    magnitude: Option<f64>,
) -> crate::server::ai::decider::types::LeadRating {
    let mut rating = fake_rating();
    if let Some(r) = relevance {
        rating.relevance = r;
    }
    if let Some(m) = magnitude {
        rating.magnitude = m;
    }
    rating
}

#[tokio::test(flavor = "current_thread")]
async fn jev_scores_decide_what_stays_and_children_of_cut_parents_go() {
    // it("후보마다 Jev로 관련도·영향도를 매겨 기준 이상만 남기고, 부모가 잘리면 자식도 자른다")
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    // HBM(1단계)만 관련도 미달 → 그 아래 엔비디아·SK하이닉스도 잘린다. 환율은 영향도 미달.
    let decider = fake_decider(|input| {
        Ok(match input.subject.text.as_str() {
            "HBM" => rating_with(Some(5.0), None),
            "환율" => rating_with(None, Some(4.9)),
            _ => fake_rating(),
        })
    });
    let at = || now();
    let result = generate_keywords_for_stock(
        &shared.db,
        &tree_llm(),
        id,
        &opts(Some(&at), Some(&decider)),
    )
    .await;
    assert_eq!(
        result,
        GenerateResult::Ok(GenerateOk {
            count: 6,
            depth: 2,
            deepest: 2,
            dropped: 0,
            candidates: 10,
            rated: 10,
            unrated_reason: None,
            headlines: 0,
        })
    );
    let rows = keyword_rows(&shared, id);
    assert_eq!(
        rows.iter().map(|r| r.text_ko.as_str()).collect::<Vec<_>>(),
        [
            "메모리 반도체",
            "스마트폰",
            "파운드리",
            "D램 가격",
            "TSMC",
            "애플"
        ]
    );
    assert_eq!(
        (rows[0].relevance, rows[0].magnitude),
        (Some(7.0), Some(6.0))
    );
    // Jev 입력: 종목 기준, 키워드 경로와 이유
    let calls = decider.rate_calls.lock().unwrap();
    let first = &calls[0].0;
    assert_eq!(first.stock.name, "삼성전자");
    assert_eq!(first.stock.code, "005930");
    assert_eq!(first.subject.text, "메모리 반도체");
    assert_eq!(
        first.subject.path.as_deref(),
        Some("삼성전자 → 메모리 반도체")
    );
    assert_eq!(first.subject.reason.as_deref(), Some("주력 사업"));
    drop(calls);
    // 평가마다 그 종목의 사용량으로 남긴다(항목별 비용).
    let rating: Vec<_> = usage_purposes(&shared)
        .into_iter()
        .filter(|r| r.0 == "keyword_rating")
        .collect();
    assert_eq!(rating.len(), 10);
    assert!(rating.iter().all(|r| r.2 == Some(id)));
}

#[tokio::test(flavor = "current_thread")]
async fn thresholds_follow_settings() {
    // it("기준은 설정 값을 따른다")
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    write_num(
        &shared.db.lock().unwrap(),
        &KEYWORDS_MIN_RELEVANCE,
        8.0,
        now(),
    )
    .unwrap();
    let decider = fake_decider(|_| Ok(fake_rating()));
    let result =
        generate_keywords_for_stock(&shared.db, &tree_llm(), id, &opts(None, Some(&decider))).await;
    // 모두 7점이라 하나도 남지 않으면 실패로 남기고 이전 키워드를 둔다.
    assert_eq!(
        result,
        GenerateResult::Err("규칙에 맞는 키워드가 나오지 않았습니다.".into())
    );
}

#[tokio::test(flavor = "current_thread")]
async fn bad_jev_key_stops_rating_and_keeps_all_candidates() {
    // it("Jev 키가 맞지 않으면 평가를 멈추고 후보를 자르지 않는다")
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    let decider = fake_decider(|_| {
        Err(DeciderError::new(
            "키가 맞지 않습니다.",
            DeciderErrorKind::Unauthorized,
        ))
    });
    let result =
        ok(
            generate_keywords_for_stock(&shared.db, &tree_llm(), id, &opts(None, Some(&decider)))
                .await,
        );
    assert_eq!((result.count, result.rated), (10, 0));
    assert_eq!(
        result.unrated_reason.as_deref(),
        Some("키가 맞지 않습니다.")
    );
    assert_eq!(decider.rate_calls.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn three_failures_in_a_row_stop_rating_and_keep_up_to_40() {
    // it("평가가 3번 연달아 실패하면 멈추고, 안전 상한(40개)까지만 남긴다")
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    let many = json!({
        "keywords": (0..50).map(|n| json!({ "level": 1, "keyword": format!("후보{n}"), "keywordEn": "", "parent": "", "reason": "r" })).collect::<Vec<_>>()
    });
    let decider = fake_decider(|_| Err(DeciderError::new("일시 오류", DeciderErrorKind::Upstream)));
    let llm = fake_llm(move |_| many.clone(), USAGE);
    let result =
        ok(generate_keywords_for_stock(&shared.db, &llm, id, &opts(None, Some(&decider))).await);
    assert_eq!(
        (result.count, result.candidates, result.rated),
        (MAX_KEPT_KEYWORDS, 50, 0)
    );
    assert_eq!(
        result.unrated_reason.as_deref(),
        Some("Jev 호출이 3번 연달아 실패했습니다.")
    );
    assert_eq!(decider.rate_calls.lock().unwrap().len(), 3);
}

#[tokio::test(flavor = "current_thread")]
async fn regenerating_keeps_branches_attached_by_topic_refresh() {
    // it("다시 만들어도 토픽 갱신이 붙인 가지는 남긴다")
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    {
        let conn = shared.db.lock().unwrap();
        let topic = seed_topic(&conn, "auto", "미중 반도체 갈등", &["수출 통제"], now());
        attach_topic(
            &conn,
            id,
            "삼성전자",
            topic,
            "미중 반도체 갈등",
            &["수출 통제"],
            now(),
        );
    }
    generate_plain(&shared, &tree_llm(), id, None).await;
    let rows = keyword_rows(&shared, id);
    let attached: Vec<&str> = rows
        .iter()
        .filter(|r| r.topic_id.is_some())
        .map(|r| r.path.as_str())
        .collect();
    assert_eq!(
        attached,
        [
            "삼성전자 → 미중 반도체 갈등",
            "삼성전자 → 미중 반도체 갈등 → 수출 통제"
        ]
    );
    assert_eq!(rows.len(), 12);
}

#[tokio::test(flavor = "current_thread")]
async fn market_item_is_built_for_the_market_without_stock_headlines() {
    // it("시장 항목은 시장 기준으로, 종목 기사 제목 없이 만든다")
    let shared = seeded();
    let market_id = seed_market(&shared.db.lock().unwrap(), "KR", now());
    let decider = fake_decider(|_| Ok(fake_rating()));
    let llm = tree_llm();
    let result =
        ok(
            generate_keywords_for_stock(&shared.db, &llm, market_id, &opts(None, Some(&decider)))
                .await,
        );
    assert_eq!(result.headlines, 0);
    assert!(
        llm.calls.lock().unwrap()[0].messages[1]
            .content
            .contains("대상: 한국 증시(코스피·코스닥) 전체")
    );
    let calls = decider.rate_calls.lock().unwrap();
    assert_eq!(
        calls[0].0.stock.kind,
        crate::server::ai::decider::types::JudgeTarget::Market
    );
    assert_eq!(calls[0].0.stock.name, "한국 증시");
}

#[tokio::test(flavor = "current_thread")]
async fn user_topic_is_built_but_auto_topic_item_is_not() {
    // it("사용자 주제는 만들고, 자동 토픽 항목은 만들지 않는다(검색어는 토픽 갱신이 정함)")
    let shared = seeded();
    let (user_lens, auto_lens) = {
        let conn = shared.db.lock().unwrap();
        seed_market(&conn, "KR", now());
        let user = seed_topic(&conn, "user", "부동산 시장", &[], now());
        let user_lens = seed_lens(&conn, user, "부동산 시장", "user", "KR", now());
        let auto = seed_topic(&conn, "auto", "중동 긴장", &["사우디"], now());
        (
            user_lens,
            seed_lens(&conn, auto, "중동 긴장", "auto", "KR", now()),
        )
    };
    let llm = tree_llm();
    assert!(matches!(
        generate_plain(&shared, &llm, user_lens, None).await,
        GenerateResult::Ok(_)
    ));
    assert!(
        llm.calls.lock().unwrap()[0].messages[1]
            .content
            .contains("주제: 부동산 시장")
    );

    assert_eq!(
        generate_plain(&shared, &llm, auto_lens, None).await,
        GenerateResult::Err("자동 토픽은 토픽 갱신이 검색어를 정합니다.".into())
    );
    assert_eq!(llm.call_count(), 1);
}

// describe("stocksAwaitingKeywords")
#[tokio::test(flavor = "current_thread")]
async fn only_pending_or_generating_stocks_in_id_order() {
    // it("삭제하지 않은 종목 중 대기·만드는 중인 것만 id 순서로")
    let shared = seeded();
    let samsung = register(&shared, Market::Kr);
    let nvidia = register(&shared, Market::Us);
    let awaiting = |s: &SharedTestDb| stocks_awaiting_keywords(&s.db.lock().unwrap()).unwrap();
    assert_eq!(awaiting(&shared), [samsung, nvidia]);

    generate_plain(&shared, &tree_llm(), samsung, None).await;
    assert_eq!(awaiting(&shared), [nvidia]);

    generate_plain(&shared, &FailingLlm(LlmErrorKind::Upstream), nvidia, None).await;
    assert_eq!(awaiting(&shared), Vec::<i64>::new());

    {
        let conn = shared.db.lock().unwrap();
        conn.execute("UPDATE stocks SET keywords_status = 'generating'", [])
            .unwrap();
        delete_stock(&conn, nvidia, now()).unwrap();
    }
    assert_eq!(awaiting(&shared), [samsung]);
}

// describe("listKeywordsByStock")
#[tokio::test(flavor = "current_thread")]
async fn keywords_are_grouped_by_stock_in_level_then_creation_order() {
    // it("종목별로 묶고 단계·생성 순서대로, 키워드가 없는 종목은 빈 목록")
    let shared = seeded();
    let samsung = register(&shared, Market::Kr);
    let nvidia = register(&shared, Market::Us);
    let reversed = fake_llm(
        |_| {
            let mut tree = sample_keyword_tree(false);
            tree["keywords"].as_array_mut().unwrap().reverse();
            tree
        },
        USAGE,
    );
    generate_plain(&shared, &reversed, samsung, None).await;

    let conn = shared.db.lock().unwrap();
    let grouped = list_keywords_by_stock(&conn, &[samsung, nvidia]).unwrap();
    let levels: Vec<i64> = grouped[&samsung].iter().map(|k| k.level).collect();
    assert_eq!(levels, [1, 1, 1, 1, 1, 2, 2, 2, 2, 2]);
    assert!(grouped[&nvidia].is_empty());
    assert_eq!(list_keywords_by_stock(&conn, &[]).unwrap().len(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn judge_item_marks_official_name_only_when_it_differs() {
    // Rust에서만 생기는 경우: `judgeItem`의 이름 규칙(한글 이름이 있고 정식 이름과 다를 때만 정식 이름을 붙임)
    let shared = seeded();
    let us = register(&shared, Market::Us);
    let conn = shared.db.lock().unwrap();
    let stock = crate::server::stocks::repository::find_stock(&conn, us)
        .unwrap()
        .unwrap();
    let item = judge_item(&stock, None);
    assert_eq!(item.name, "엔비디아");
    assert_eq!(
        item.official_name.as_deref(),
        Some("NVIDIA Corporation - Common Stock")
    );
    let mut same = stock.clone();
    same.name_ko = Some(same.name.clone());
    assert_eq!(judge_item(&same, None).official_name, None);
    let mut none = stock;
    none.name_ko = None;
    assert_eq!(
        judge_item(&none, Some("요약")).summary.as_deref(),
        Some("요약")
    );
}
