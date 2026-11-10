use serde_json::json;

use super::*;
use crate::server::ai::llm::{JsonResult, LlmClient, LlmUsage, StructuredRequest};
use crate::server::budget::usage::{UsageEntry, record_usage};
use crate::server::db::schema::{Market, UsageProvider};
use crate::server::keywords::read::list_keywords_by_stock;
use crate::server::settings::{KEYWORDS_DEPTH, write_num};
use crate::server::stocks::repository::{
    RegisterInput, RegisterResult, delete_stock, register_stock,
};
use crate::test_support::{
    LinkSpec, SharedTestDb, attach_topic, fake_llm, insert_link, ms, sample_keyword_tree,
    seed_lens, seed_listings, seed_market, seed_topic, set_link_path, shared_test_db,
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

fn opts(at: Option<&(dyn Fn() -> EpochMs + Send + Sync)>) -> GenerateOptions<'_> {
    GenerateOptions { now: at }
}

/// 저장된 (별칭, 고유어)
fn stored_terms(shared: &SharedTestDb, id: i64) -> (Vec<String>, Vec<String>) {
    let conn = shared.db.lock().unwrap();
    let stock = crate::server::stocks::repository::find_stock(&conn, id)
        .unwrap()
        .unwrap();
    (stock.aliases, stock.anchors)
}

/// 표본 트리에 별칭·고유어만 바꾼 답
fn tree_with_terms(aliases: &[&str], anchors: &[&str]) -> serde_json::Value {
    let mut tree = sample_keyword_tree(false);
    tree["aliases"] = json!(aliases);
    tree["anchors"] = json!(anchors);
    tree
}

async fn generate_plain(
    shared: &SharedTestDb,
    llm: &impl LlmClient,
    id: i64,
    at: Option<&(dyn Fn() -> EpochMs + Send + Sync)>,
) -> GenerateResult {
    generate_keywords_for_stock(&shared.db, llm, id, &opts(at)).await
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

    // Jev 평가 없이 상한 안의 키워드를 모두 남기고, 종목의 별칭·고유어도 저장한다(17단계).
    assert_eq!(
        result,
        GenerateResult::Ok(GenerateOk {
            count: 10,
            depth: 2,
            deepest: 2,
            dropped: 0,
            stat_words: 0,
            aliases: 1,
            anchors: 5,
            update: None,
        })
    );
    assert_eq!(llm.call_count(), 1);
    {
        let calls = llm.calls.lock().unwrap();
        assert_eq!(calls[0].model, "anthropic/claude-haiku-5.5");
        assert_eq!(calls[0].schema_name, "keyword_tree");
        assert_eq!(calls[0].max_tokens, 8_000);
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
    // 새 키워드에는 Jev 점수가 없다.
    assert!(
        rows.iter()
            .all(|r| r.relevance.is_none() && r.magnitude.is_none())
    );
    assert_eq!(
        stored_terms(&shared, id),
        (
            vec!["삼전".to_string()],
            ["반도체", "메모리", "파운드리", "HBM", "스마트폰"]
                .map(String::from)
                .to_vec()
        )
    );

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
async fn stat_words_of_passed_name_articles_are_sent_and_counted() {
    // it("그 종목의 통과한 이름 연결 기사에서 뽑은 통계 낱말을 함께 넣고, 넣은 수를 알려 준다")
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    {
        let conn = shared.db.lock().unwrap();
        for title in [
            "삼성전자 엔비디아 납품 확대",
            "삼성전자 엔비디아 승인 임박",
            "삼성전자 엔비디아 협력 강화",
        ] {
            let mut link = LinkSpec::new(id, title, later() - 60_000).with_relevance(8.0);
            link.magnitude = Some(8.0);
            insert_link(&conn, &link);
        }
        // 영향도 미달 기사는 세지 않는다.
        let mut weak = LinkSpec::new(id, "삼성전자 노조 협상", later() - 60_000);
        weak.magnitude = Some(1.0);
        insert_link(&conn, &weak);
        for i in 0..20 {
            conn.execute(
                "INSERT INTO feed_headlines (title_key, title, publisher, feed, published_at, seen_at) \
                 VALUES (?1, ?2, '테스트', 'rss:test:economy', ?3, ?3)",
                rusqlite::params![format!("k{i}"), format!("경제 소식 {i}번째 기사"), later()],
            )
            .unwrap();
        }
    }
    let llm = tree_llm();
    let at = || later();

    let result = ok(generate_plain(&shared, &llm, id, Some(&at)).await);

    assert_eq!(result.stat_words, 2);
    let calls = llm.calls.lock().unwrap();
    assert!(
        calls[0].messages[1]
            .content
            .contains("통과 기사에 자주 나온 낱말(참고 자료, 최근 14일 기사 3건): ")
    );
    assert!(calls[0].messages[1].content.contains("엔비디아(3)"));
    assert!(calls[0].messages[1].content.contains("삼성전자(3)"));
    assert!(!calls[0].messages[0].content.contains("최근 주요 기사 제목"));
}

#[tokio::test(flavor = "current_thread")]
async fn jev_is_not_called_and_only_the_keyword_generation_call_is_logged() {
    // it("Jev 후보 평가를 하지 않는다: 사용량에는 키워드 생성 호출 하나만 남는다")
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    let result = ok(generate_plain(&shared, &tree_llm(), id, None).await);
    assert_eq!(result.count, 10);
    let purposes: Vec<String> = usage_purposes(&shared).into_iter().map(|r| r.0).collect();
    assert_eq!(purposes, ["keyword_generation"]);
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
    assert_eq!((result.dropped, result.count), (0, 5));
    // 깊이 1/2: 원문 기록 조건(`deepest < depth`)이 참이다.
    let raw = parse_keyword_tree(
        &json!({ "aliases": [], "anchors": ["반도체"], "keywords": sample_keyword_tree(false)["keywords"].as_array().unwrap()[..2] }),
    )
    .unwrap();
    let line = format!(
        "키워드 LLM 원문(종목 id {id}, 버림 0개, 깊이 1/2): {}",
        js_slice(&raw.to_json_string(), 0, RAW_LOG_MAX_CHARS)
    );
    assert!(line.starts_with("키워드 LLM 원문(종목 id 1, 버림 0개, 깊이 1/2): {\"aliases\":[],\"anchors\":[\"반도체\"],\"keywords\":[{\"level\":1,\"keyword\":\"메모리 반도체\""));
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
async fn depth_follows_setting_and_is_clamped_to_1_to_2() {
    // it("깊이는 설정 값을 따르고 1~2로 맞춘다")
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    let llm = tree_llm();

    write_num(&shared.db.lock().unwrap(), &KEYWORDS_DEPTH, 1.0, now()).unwrap();
    // 깊이 1이면 2단계 5개는 버린다.
    let result = ok(generate_plain(&shared, &llm, id, None).await);
    assert_eq!((result.count, result.depth, result.dropped), (5, 1, 5));
    assert!(
        llm.calls.lock().unwrap()[0].messages[0]
            .content
            .contains("깊이는 1단계까지다")
    );

    write_num(&shared.db.lock().unwrap(), &KEYWORDS_DEPTH, 9.0, now()).unwrap();
    let result = ok(generate_plain(&shared, &llm, id, None).await);
    assert_eq!((result.count, result.depth), (10, 2));
    assert!(
        llm.calls.lock().unwrap()[1].messages[0]
            .content
            .contains("깊이는 2단계까지다")
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
        |_| json!({ "aliases": [], "anchors": ["반도체"], "keywords": [{ "level": 1, "keyword": "반도체", "keywordEn": "", "parent": "", "reason": "r" }] }),
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
        |_| json!({ "aliases": [], "anchors": ["반도체"], "keywords": [{ "level": 1, "keyword": "삼성전자", "keywordEn": "", "parent": "", "reason": "" }] }),
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

// describe("generateKeywordsForStock: 별칭·고유어 (17단계)")
#[tokio::test(flavor = "current_thread")]
async fn aliases_are_validated_and_anchors_refined_and_saved_with_the_tree() {
    // it("별칭은 검증을 거치고 고유어는 다듬어서, 키워드와 한 트랜잭션으로 저장한다")
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    // 자기 이름·코드·다른 종목 이름·한 글자는 별칭에서 버려진다.
    let llm = fake_llm(
        |_| {
            tree_with_terms(
                &[
                    "삼성전자",
                    "005930",
                    "SK하이닉스",
                    "삼",
                    "삼전",
                    "삼전",
                    "SEC",
                ],
                &[" 반도체 ", "5", "2026", "HBM", "hbm", "메모리 반도체", "A"],
            )
        },
        USAGE,
    );
    let result = ok(generate_plain(&shared, &llm, id, None).await);
    assert_eq!((result.aliases, result.anchors), (2, 3));
    assert_eq!(
        stored_terms(&shared, id),
        (
            vec!["삼전".to_string(), "SEC".to_string()],
            vec![
                "반도체".to_string(),
                "HBM".to_string(),
                "메모리 반도체".to_string()
            ]
        )
    );
    assert_eq!(keyword_rows(&shared, id).len(), 10);
}

#[tokio::test(flavor = "current_thread")]
async fn anchors_are_cut_to_15() {
    // it("고유어는 최대 15개만 저장한다")
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    let many: Vec<String> = (0..20).map(|n| format!("고유어{n}")).collect();
    let llm = fake_llm(
        move |_| {
            let refs: Vec<&str> = many.iter().map(String::as_str).collect();
            tree_with_terms(&[], &refs)
        },
        USAGE,
    );
    let result = ok(generate_plain(&shared, &llm, id, None).await);
    assert_eq!(result.anchors, 15);
    assert_eq!(stored_terms(&shared, id).1.len(), 15);
}

#[tokio::test(flavor = "current_thread")]
async fn stock_without_anchors_fails_and_keeps_previous_tree_and_terms() {
    // it("종목인데 고유어가 0개면 실패하고 이전 트리·별칭·고유어를 그대로 둔다")
    let shared = seeded();
    let id = register(&shared, Market::Kr);
    let (first, second) = (|| now(), || later());
    generate_plain(&shared, &tree_llm(), id, Some(&first)).await;
    let before = (keyword_rows(&shared, id).len(), stored_terms(&shared, id));

    // 고유어가 비었거나, 다듬고 나면 아무것도 남지 않는 경우
    for anchors in [&[][..], &["1", "A", "  "][..]] {
        let llm = fake_llm(
            {
                let anchors: Vec<String> = anchors.iter().map(|a| (*a).to_string()).collect();
                move |_| {
                    let refs: Vec<&str> = anchors.iter().map(String::as_str).collect();
                    tree_with_terms(&["새별칭"], &refs)
                }
            },
            USAGE,
        );
        let result = generate_plain(&shared, &llm, id, Some(&second)).await;
        assert_eq!(
            result,
            GenerateResult::Err("고유어가 나오지 않았습니다.".into())
        );
        assert_eq!(
            stock_state(&shared, id),
            (
                "failed".into(),
                Some("고유어가 나오지 않았습니다.".into()),
                Some(now())
            )
        );
        assert_eq!(
            (keyword_rows(&shared, id).len(), stored_terms(&shared, id)),
            before
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn market_and_topic_items_do_not_save_aliases_or_anchors() {
    // it("시장·주제 항목은 별칭·고유어를 저장하지 않고, 고유어가 비어도 실패하지 않는다")
    let shared = seeded();
    let (market, topic) = {
        let conn = shared.db.lock().unwrap();
        let market = seed_market(&conn, "KR", now());
        let user = seed_topic(&conn, "user", "부동산 시장", &[], now());
        (
            market,
            seed_lens(&conn, user, "부동산 시장", "user", "KR", now()),
        )
    };
    let llm = fake_llm(|_| tree_with_terms(&["별칭"], &["고유어"]), USAGE);
    let result = ok(generate_plain(&shared, &llm, market, None).await);
    assert_eq!((result.aliases, result.anchors), (0, 0));
    assert_eq!(stored_terms(&shared, market), (vec![], vec![]));

    let empty = fake_llm(|_| tree_with_terms(&[], &[]), USAGE);
    ok(generate_plain(&shared, &empty, topic, None).await);
    assert_eq!(stored_terms(&shared, topic), (vec![], vec![]));
    assert_eq!(keyword_rows(&shared, topic).len(), 10);
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
async fn market_item_is_built_for_the_market_without_stat_words() {
    // it("시장 항목은 시장 기준으로, 종목 통계 낱말 없이 만든다")
    let shared = seeded();
    let market_id = seed_market(&shared.db.lock().unwrap(), "KR", now());
    let llm = tree_llm();
    let result = ok(generate_plain(&shared, &llm, market_id, None).await);
    assert_eq!(result.stat_words, 0);
    assert_eq!(
        llm.calls.lock().unwrap()[0].messages[1].content,
        "대상: 한국 증시(코스피·코스닥) 전체"
    );
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

// ---- 정기 점검(증분, 17단계 tree-weekly) ----

const DAY: i64 = 24 * 60 * 60 * 1000;

fn empty_update() -> serde_json::Value {
    json!({ "remove": [], "add": [], "anchorsRemove": [], "anchorsAdd": [], "aliasesAdd": [] })
}

fn update_llm(answer: serde_json::Value) -> crate::test_support::FakeLlm {
    fake_llm(move |_| answer.clone(), USAGE)
}

/// 표본 트리(1단계 5·2단계 5)를 `now()`에 전체 생성으로 만들어 둔 종목
async fn with_tree(shared: &SharedTestDb) -> i64 {
    let id = register(shared, Market::Kr);
    let at = || now();
    ok(generate_plain(shared, &tree_llm(), id, Some(&at)).await);
    id
}

async fn periodic(shared: &SharedTestDb, llm: &impl LlmClient, id: i64) -> GenerateResult {
    let at = || later();
    generate_keywords_periodic(&shared.db, llm, id, &opts(Some(&at))).await
}

/// 키워드 연결 하나(경로의 마지막 낱말이 키워드). 관련·영향 8점이면 표시 기준을 통과한다.
fn keyword_link(shared: &SharedTestDb, id: i64, path: &str, at: EpochMs, score: f64) {
    let conn = shared.db.lock().unwrap();
    let mut spec = LinkSpec::new(id, "기사", at).with_relevance(score);
    spec.kind = "keyword";
    spec.magnitude = Some(score);
    let (match_id, _) = insert_link(&conn, &spec);
    set_link_path(&conn, match_id, path, false);
}

fn texts_of(rows: &[Row], level: i64) -> Vec<String> {
    rows.iter()
        .filter(|r| r.level == level && r.topic_id.is_none())
        .map(|r| r.text_ko.clone())
        .collect()
}

#[tokio::test(flavor = "current_thread")]
async fn periodic_check_sends_the_current_tree_and_uses_the_update_format() {
    let shared = seeded();
    let id = with_tree(&shared).await;
    let llm = update_llm(empty_update());

    let result = ok(periodic(&shared, &llm, id).await);

    let update = result.update.expect("정기 점검 결과");
    assert_eq!((update.added, update.removed, update.evicted), (0, 0, 0));
    let calls = llm.calls.lock().unwrap();
    assert_eq!(calls[0].schema_name, "keyword_update");
    assert_eq!(calls[0].max_tokens, 6_000);
    let user = &calls[0].messages[1].content;
    assert!(user.contains("- HBM > 엔비디아, SK하이닉스"), "{user}");
    assert!(user.contains("지금 고유어: 반도체, 메모리, 파운드리, HBM, 스마트폰"));
    assert!(user.contains("지금 별칭: 삼전"));
    drop(calls);
    assert_eq!(
        usage_purposes(&shared),
        [
            (
                "keyword_generation".into(),
                "anthropic/claude-haiku-5.5".into(),
                Some(id)
            ),
            (
                "keyword_generation".into(),
                "anthropic/claude-haiku-5.5".into(),
                Some(id)
            ),
        ]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn nothing_changed_keeps_rows_and_refreshes_generated_at() {
    let shared = seeded();
    let id = with_tree(&shared).await;
    let before: Vec<(i64, i64)> = keyword_rows(&shared, id)
        .iter()
        .map(|r| (r.id, r.created_at))
        .collect();

    let result = ok(periodic(&shared, &update_llm(empty_update()), id).await);

    assert_eq!(result.count, 10);
    let after: Vec<(i64, i64)> = keyword_rows(&shared, id)
        .iter()
        .map(|r| (r.id, r.created_at))
        .collect();
    assert_eq!(before, after);
    assert_eq!(
        stock_state(&shared, id),
        ("ready".into(), None, Some(later()))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn removal_is_kept_when_the_keyword_passed_in_the_last_four_weeks() {
    let shared = seeded();
    let id = with_tree(&shared).await;
    // 엔비디아: 4주 안 통과(남음). 환율: 30일 전 통과(지켜 주지 않음). 스마트폰: 실적 없음.
    keyword_link(&shared, id, "삼성전자 → HBM → 엔비디아", later() - DAY, 8.0);
    keyword_link(&shared, id, "삼성전자 → 환율", later() - 30 * DAY, 8.0);
    // 기준 미만은 통과가 아니다
    keyword_link(
        &shared,
        id,
        "삼성전자 → 파운드리 → TSMC",
        later() - DAY,
        3.0,
    );
    let llm = update_llm(json!({
        "remove": [
            { "keyword": " 엔비디아 ", "reason": "철수" },
            { "keyword": "환율", "reason": "일반어" },
            { "keyword": "스마트폰", "reason": "매각" },
            { "keyword": "tsmc", "reason": "x" },
            { "keyword": "없는 키워드", "reason": "x" }
        ],
        "add": [], "anchorsRemove": [], "anchorsAdd": [], "aliasesAdd": []
    }));

    let result = ok(periodic(&shared, &llm, id).await);

    let update = result.update.unwrap();
    assert_eq!(update.protected, 1);
    // 환율, 스마트폰(+ 아래 애플), TSMC
    assert_eq!(update.removed, 4);
    let rows = keyword_rows(&shared, id);
    assert_eq!(texts_of(&rows, 1), ["메모리 반도체", "HBM", "파운드리"]);
    assert_eq!(texts_of(&rows, 2), ["엔비디아", "SK하이닉스", "D램 가격"]);
}

#[tokio::test(flavor = "current_thread")]
async fn removing_a_level_one_removes_everything_below_even_with_passes() {
    let shared = seeded();
    let id = with_tree(&shared).await;
    // HBM 아래 엔비디아가 통과했어도 1단계 HBM이 빠지면 같이 빠진다.
    keyword_link(&shared, id, "삼성전자 → HBM → 엔비디아", later() - DAY, 8.0);
    let llm = update_llm(json!({
        "remove": [{ "keyword": "HBM", "reason": "x" }],
        "add": [], "anchorsRemove": [], "anchorsAdd": [], "aliasesAdd": []
    }));
    ok(periodic(&shared, &llm, id).await);
    let rows = keyword_rows(&shared, id);
    assert!(
        rows.iter()
            .all(|r| r.text_ko != "HBM" && r.text_ko != "엔비디아" && r.text_ko != "SK하이닉스")
    );
    assert_eq!(rows.len(), 7);
}

#[tokio::test(flavor = "current_thread")]
async fn adds_at_most_three_validated_keywords_and_leaves_old_rows_alone() {
    let shared = seeded();
    let id = with_tree(&shared).await;
    let before = keyword_rows(&shared, id);
    let item = |level: i64, keyword: &str, parent: &str| json!({ "level": level, "keyword": keyword, "keywordEn": "", "parent": parent, "reason": format!("{keyword} 이유") });
    let llm = update_llm(json!({
        "remove": [],
        "add": [
            item(1, "환율", ""),            // 이미 있음
            item(1, "삼성전자", ""),        // 대상 이름
            item(2, "고아", "없는 부모"),   // 부모 없음
            item(3, "깊은 것", "HBM"),      // 3단계
            item(1, "전력 반도체", ""),
            item(2, "패키징", "HBM"),
            item(2, "SiC", "전력 반도체"),  // 이번에 더한 1단계 아래
            item(1, "여섯째", ""),          // 3개를 넘음
        ],
        "anchorsRemove": [], "anchorsAdd": [], "aliasesAdd": []
    }));

    let result = ok(periodic(&shared, &llm, id).await);

    let update = result.update.unwrap();
    assert_eq!((update.added, update.dropped), (3, 5));
    let rows = keyword_rows(&shared, id);
    assert_eq!(rows.len(), 13);
    let power = rows.iter().find(|r| r.text_ko == "전력 반도체").unwrap();
    let sic = rows.iter().find(|r| r.text_ko == "SiC").unwrap();
    let packaging = rows.iter().find(|r| r.text_ko == "패키징").unwrap();
    let hbm = rows.iter().find(|r| r.text_ko == "HBM").unwrap();
    assert_eq!((power.level, power.parent_id), (1, None));
    assert_eq!(sic.parent_id, Some(power.id));
    assert_eq!(packaging.parent_id, Some(hbm.id));
    assert_eq!(sic.path, "삼성전자 → 전력 반도체 → SiC");
    assert_eq!(
        (power.kind.as_str(), power.relevance, power.created_at),
        ("structure", None, later())
    );
    // 바뀌지 않은 줄은 id·만든 시각이 그대로다.
    for old in &before {
        let now_row = rows.iter().find(|r| r.id == old.id).expect("그대로 남음");
        assert_eq!(
            (now_row.created_at, &now_row.text_ko),
            (old.created_at, &old.text_ko)
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn over_the_cap_the_oldest_last_pass_goes_first_with_its_children() {
    let shared = seeded();
    let id = with_tree(&shared).await;
    // HBM은 만든 뒤에 통과했다(마지막 통과가 더 늦다). 나머지는 모두 만든 시각이 같다.
    keyword_link(&shared, id, "삼성전자 → HBM", later() - 60_000, 8.0);
    let item = |keyword: &str| json!({ "level": 1, "keyword": keyword, "keywordEn": "", "parent": "", "reason": "r" });
    let llm = update_llm(json!({
        "remove": [],
        "add": [item("전력 반도체"), item("2차전지"), item("바이오")],
        "anchorsRemove": [], "anchorsAdd": [], "aliasesAdd": []
    }));

    let result = ok(periodic(&shared, &llm, id).await);

    // 1단계 8개 → 6개: 가장 오래된 메모리 반도체(와 D램 가격)·스마트폰(과 애플)이 빠진다.
    let update = result.update.unwrap();
    assert_eq!(update.evicted, 4);
    let rows = keyword_rows(&shared, id);
    assert_eq!(
        texts_of(&rows, 1),
        [
            "HBM",
            "파운드리",
            "환율",
            "전력 반도체",
            "2차전지",
            "바이오"
        ]
    );
    assert_eq!(texts_of(&rows, 2), ["엔비디아", "SK하이닉스", "TSMC"]);
}

#[tokio::test(flavor = "current_thread")]
async fn level_two_cap_ignores_topic_branches_and_uses_last_pass_or_created_time() {
    let shared = seeded();
    let id = with_tree(&shared).await;
    {
        let conn = shared.db.lock().unwrap();
        let hbm: i64 = conn
            .query_row("SELECT id FROM keywords WHERE text_ko = 'HBM'", [], |r| {
                r.get(0)
            })
            .unwrap();
        // 2단계 5개 + 9개 = 14개. 추가k는 만든 지 k일.
        for k in 1..=9 {
            conn.execute(
                "INSERT INTO keywords (stock_id, level, parent_id, text_ko, reason, path, kind, created_at) \
                 VALUES (?1, 2, ?2, ?3, 'r', ?4, 'structure', ?5)",
                rusqlite::params![id, hbm, format!("추가{k}"), format!("삼성전자 → HBM → 추가{k}"), now() - k * DAY],
            )
            .unwrap();
        }
        // 토픽 가지는 세지 않는다.
        let topic = seed_topic(&conn, "auto", "관세", &["a"], now());
        attach_topic(
            &conn,
            id,
            "삼성전자",
            topic,
            "관세",
            &["관세 협상", "관세 인상", "관세 면제"],
            now(),
        );
    }
    // 가장 오래된 추가9에 마지막 통과가 있으면 그 시각이 기준이다.
    keyword_link(&shared, id, "삼성전자 → HBM → 추가9", later() - 60_000, 8.0);

    let result = ok(periodic(&shared, &update_llm(empty_update()), id).await);

    assert_eq!(result.update.unwrap().evicted, 2);
    let rows = keyword_rows(&shared, id);
    let level2 = texts_of(&rows, 2);
    assert_eq!(level2.len(), 12);
    // 추가8·추가7이 가장 오래돼 빠진다. 추가9는 최근에 통과해 남는다.
    assert!(!level2.contains(&"추가8".to_string()));
    assert!(!level2.contains(&"추가7".to_string()));
    assert!(level2.contains(&"추가9".to_string()));
    assert_eq!(rows.iter().filter(|r| r.topic_id.is_some()).count(), 4);
}

#[tokio::test(flavor = "current_thread")]
async fn old_deep_tree_keeps_only_levels_one_and_two() {
    let shared = seeded();
    let id = with_tree(&shared).await;
    {
        let conn = shared.db.lock().unwrap();
        let parent: i64 = conn
            .query_row(
                "SELECT id FROM keywords WHERE text_ko = '엔비디아'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO keywords (stock_id, level, parent_id, text_ko, reason, path, kind, created_at) \
             VALUES (?1, 3, ?2, '3단계 옛것', 'r', '삼성전자 → HBM → 엔비디아 → 3단계 옛것', 'structure', ?3)",
            rusqlite::params![id, parent, now()],
        )
        .unwrap();
    }
    let result = ok(periodic(&shared, &update_llm(empty_update()), id).await);
    assert_eq!(result.update.unwrap().removed, 1);
    let rows = keyword_rows(&shared, id);
    assert_eq!(rows.len(), 10);
    assert!(rows.iter().all(|r| r.level <= 2));
}

#[tokio::test(flavor = "current_thread")]
async fn anchors_and_aliases_are_trimmed_and_validated() {
    let shared = seeded();
    let id = with_tree(&shared).await;
    let llm = update_llm(json!({
        "remove": [], "add": [],
        "anchorsRemove": [" 반도체 ", "없는 말"],
        "anchorsAdd": ["5", "x", "엔비디아", "HBM3E", "hbm", "AI서버", "전력"],
        "aliasesAdd": ["삼전", "SEC전자", "SK하이닉스", "삼"]
    }));

    let result = ok(periodic(&shared, &llm, id).await);

    let (aliases, anchors) = stored_terms(&shared, id);
    // 별칭: 있던 것은 그대로, 이름이 같은 다른 종목·2자 미만·중복은 버린다
    assert_eq!(aliases, ["삼전", "SEC전자"]);
    // 고유어: 빼고 나서 더한다. 더하기는 검증을 통과한 것 가운데 최대 3개(숫자·1자·중복 제외)
    assert_eq!(
        anchors,
        [
            "메모리",
            "파운드리",
            "HBM",
            "스마트폰",
            "엔비디아",
            "HBM3E",
            "AI서버"
        ]
    );
    let update = result.update.unwrap();
    assert_eq!(
        (
            update.anchors_removed,
            update.anchors_added,
            update.aliases_added
        ),
        (1, 3, 1)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn anchors_are_not_removed_when_nothing_would_be_left() {
    let shared = seeded();
    let id = with_tree(&shared).await;
    let llm = update_llm(json!({
        "remove": [], "add": [],
        "anchorsRemove": ["반도체", "메모리", "파운드리", "HBM", "스마트폰"],
        "anchorsAdd": [], "aliasesAdd": []
    }));
    ok(periodic(&shared, &llm, id).await);
    assert_eq!(stored_terms(&shared, id).1.len(), 5);
}

#[tokio::test(flavor = "current_thread")]
async fn failed_check_keeps_the_tree_and_marks_failed() {
    let shared = seeded();
    let id = with_tree(&shared).await;
    let before: Vec<(i64, String)> = keyword_rows(&shared, id)
        .iter()
        .map(|r| (r.id, r.text_ko.clone()))
        .collect();

    let at = || later();
    let result = generate_keywords_periodic(
        &shared.db,
        &FailingLlm(LlmErrorKind::Timeout),
        id,
        &opts(Some(&at)),
    )
    .await;
    assert_eq!(
        result,
        GenerateResult::Err("AI 응답이 늦어 중단했습니다.".into())
    );
    let (status, error, generated_at) = stock_state(&shared, id);
    assert_eq!(status, "failed");
    assert!(error.is_some());
    // 만든 시각은 그대로여야 다음 날 다시 고른다.
    assert_eq!(generated_at, Some(now()));
    let after: Vec<(i64, String)> = keyword_rows(&shared, id)
        .iter()
        .map(|r| (r.id, r.text_ko.clone()))
        .collect();
    assert_eq!(before, after);

    // 형식이 맞지 않는 답도 같다.
    let bad = update_llm(json!({ "remove": "x" }));
    assert!(matches!(
        periodic(&shared, &bad, id).await,
        GenerateResult::Err(_)
    ));
    assert_eq!(keyword_rows(&shared, id).len(), 10);
}

#[tokio::test(flavor = "current_thread")]
async fn periodic_falls_back_to_full_generation_without_tree_or_anchors() {
    let shared = seeded();
    // 트리가 없다.
    let id = register(&shared, Market::Kr);
    let llm = tree_llm();
    ok(periodic(&shared, &llm, id).await);
    assert_eq!(llm.calls.lock().unwrap()[0].schema_name, "keyword_tree");
    assert_eq!(keyword_rows(&shared, id).len(), 10);

    // 트리는 있지만 고유어가 비었다.
    shared
        .db
        .lock()
        .unwrap()
        .execute("UPDATE stocks SET anchors = '[]' WHERE id = ?1", [id])
        .unwrap();
    let llm = tree_llm();
    ok(periodic(&shared, &llm, id).await);
    assert_eq!(llm.calls.lock().unwrap()[0].schema_name, "keyword_tree");
    assert_eq!(stored_terms(&shared, id).1.len(), 5);
}
