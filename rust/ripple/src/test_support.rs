//! 테스트 공용 도구(`tests/db.ts`·`tests/global-setup.ts` 대응).
//! 실제 마이그레이션으로 템플릿 DB를 한 번 만들고, 테스트마다 복사해 쓴다.
use std::path::PathBuf;
use std::sync::OnceLock;

use rusqlite::Connection;
use tempfile::TempDir;

use crate::server::db::client::{DbAccess, open_db};
use crate::server::db::migrate::apply_migrations;

fn template_bytes() -> &'static [u8] {
    static TEMPLATE: OnceLock<Vec<u8>> = OnceLock::new();
    TEMPLATE.get_or_init(|| {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("template.db");
        apply_migrations(&path).unwrap();
        std::fs::read(&path).unwrap()
    })
}

/// 마이그레이션이 끝난 테스트 전용 DB 파일. 값이 버려지면 폴더째 지워진다.
pub struct TestDbFile {
    pub path: PathBuf,
    pub dir: PathBuf,
    _temp: TempDir,
}

pub fn test_db_file() -> TestDbFile {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("ripple.db");
    std::fs::write(&path, template_bytes()).unwrap();
    TestDbFile {
        path,
        dir: temp.path().to_path_buf(),
        _temp: temp,
    }
}

/// 쓰기 연결이 열린 테스트 DB
pub struct TestDb {
    pub conn: Connection,
    _file: TestDbFile,
}

pub fn test_db() -> TestDb {
    let file = test_db_file();
    let conn = open_db(&file.path, DbAccess::ReadWrite).unwrap();
    TestDb { conn, _file: file }
}

/// 여러 곳에서 나눠 쓰는 연결(`Db`)이 열린 테스트 DB. 값이 버려지면 폴더째 지워진다.
pub struct SharedTestDb {
    pub db: crate::server::db::client::Db,
    _file: TestDbFile,
}

pub fn shared_test_db() -> SharedTestDb {
    let TestDb { conn, _file } = test_db();
    SharedTestDb {
        db: crate::server::db::client::into_db(conn),
        _file,
    }
}

use std::sync::Mutex;

use serde_json::Value;

use crate::server::ai::llm::{
    JsonResult, LlmClient, LlmError, LlmMessage, LlmUsage, StructuredRequest,
};
use crate::server::db::schema::UsageProvider;

/// `FakeLlm`이 받은 요청(`tests/fixtures.ts`의 `fakeLlm().calls` 대응)
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedCall {
    pub model: String,
    pub messages: Vec<LlmMessage>,
    pub schema_name: String,
    pub max_tokens: u32,
}

type Respond = Box<dyn Fn(&[LlmMessage]) -> Value + Send + Sync>;

/// 정해진 값을 돌려주는 가짜 LLM. 받은 요청을 `calls`에 남긴다. 응답이 형식에 안 맞으면
/// `generate_object`가 `invalid_output`(사용량 포함)으로 만든다.
pub struct FakeLlm {
    respond: Respond,
    usage: LlmUsage,
    pub calls: Mutex<Vec<RecordedCall>>,
}

pub fn fake_llm(
    respond: impl Fn(&[LlmMessage]) -> Value + Send + Sync + 'static,
    usage: LlmUsage,
) -> FakeLlm {
    FakeLlm {
        respond: Box::new(respond),
        usage,
        calls: Mutex::new(Vec::new()),
    }
}

impl FakeLlm {
    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

impl LlmClient for FakeLlm {
    fn provider(&self) -> UsageProvider {
        UsageProvider::Openrouter
    }

    async fn generate_json<T: Send>(
        &self,
        request: &StructuredRequest<T>,
    ) -> Result<JsonResult, LlmError> {
        self.calls.lock().unwrap().push(RecordedCall {
            model: request.model.clone(),
            messages: request.messages.clone(),
            schema_name: request.schema_name.clone(),
            max_tokens: request.max_tokens,
        });
        Ok(JsonResult {
            json: Some((self.respond)(&request.messages)),
            usage: self.usage,
            model: request.model.clone(),
        })
    }
}

/// ISO 8601 UTC 글자("2026-10-20T03:00:00Z") → 밀리초. TS 테스트의 `new Date(iso)`.
pub fn ms(iso: &str) -> crate::server::time::EpochMs {
    chrono::DateTime::parse_from_rfc3339(iso)
        .unwrap()
        .timestamp_millis()
}

/// 실제 KIND·NASDAQ 파일에서 고른 표본(KIND는 원본처럼 EUC-KR). `rust/ripple/tests/fixtures`(TS 시절 `tests/fixtures`에서 옮김)를 읽는다.
pub struct FixtureFiles {
    pub kind: &'static str,
    pub nasdaq_listed: &'static str,
    pub other_listed: &'static str,
}

pub const FIXTURE_FILES: FixtureFiles = FixtureFiles {
    kind: "kind-sample.xls",
    nasdaq_listed: "nasdaqlisted-sample.txt",
    other_listed: "otherlisted-sample.txt",
};

pub fn fixture_bytes(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

pub fn fixture_text(name: &str) -> String {
    String::from_utf8(fixture_bytes(name)).unwrap()
}

pub fn decode_euc_kr(name: &str) -> String {
    encoding_rs::EUC_KR
        .decode_without_bom_handling(&fixture_bytes(name))
        .0
        .into_owned()
}

/// 표본으로 공식 목록을 채운다(한국 7개, 미국 9개).
pub fn seed_listings(conn: &mut Connection) {
    use crate::server::db::schema::Market;
    use crate::server::listings::parse::{
        parse_kind_list, parse_nasdaq_listed, parse_other_listed,
    };
    use crate::server::listings::sync::replace_listings;

    replace_listings(
        conn,
        Market::Kr,
        &parse_kind_list(&decode_euc_kr(FIXTURE_FILES.kind)).unwrap(),
    )
    .unwrap();
    let mut us = parse_nasdaq_listed(&fixture_text(FIXTURE_FILES.nasdaq_listed)).unwrap();
    us.extend(parse_other_listed(&fixture_text(FIXTURE_FILES.other_listed)).unwrap());
    replace_listings(conn, Market::Us, &us).unwrap();
}

/// 깊이 2 키워드 트리 표본(LLM 출력 형식: 별칭·고유어와 구조 키워드). 단계마다 5개
pub fn sample_keyword_tree(english: bool) -> Value {
    let en = |text: &str| {
        if english {
            text.to_string()
        } else {
            String::new()
        }
    };
    let level1 = [
        ("메모리 반도체", "Memory chips", "주력 사업"),
        ("HBM", "HBM", "AI 서버용 고대역폭 메모리"),
        ("스마트폰", "Smartphones", "완제품 사업"),
        ("파운드리", "Foundry", "위탁 생산 사업"),
        ("환율", "Exchange rate", "수출 비중이 커 실적에 영향"),
    ];
    let level2 = [
        ("엔비디아", "Nvidia", "HBM", "HBM 주요 고객"),
        ("SK하이닉스", "SK hynix", "HBM", "HBM 경쟁사"),
        (
            "D램 가격",
            "DRAM prices",
            "메모리 반도체",
            "메모리 실적을 좌우",
        ),
        ("TSMC", "TSMC", "파운드리", "파운드리 1위 경쟁사"),
        ("애플", "Apple", "스마트폰", "스마트폰 경쟁사"),
    ];
    let mut keywords: Vec<Value> = level1
        .iter()
        .map(|(keyword, keyword_en, reason)| {
            serde_json::json!({ "level": 1, "keyword": keyword, "keywordEn": en(keyword_en), "parent": "", "reason": reason })
        })
        .collect();
    keywords.extend(level2.iter().map(|(keyword, keyword_en, parent, reason)| {
        serde_json::json!({ "level": 2, "keyword": keyword, "keywordEn": en(keyword_en), "parent": parent, "reason": reason })
    }));
    serde_json::json!({
        "aliases": ["삼전"],
        "anchors": ["반도체", "메모리", "파운드리", "HBM", "스마트폰"],
        "keywords": keywords
    })
}

use crate::server::ai::decider::types::{
    DeciderError, Decision, DecisionInput, LeadInput, LeadRating, MarketVerdict, MergedDecision,
    MergedInput, Scored, TokenUsage,
};

/// 기본 키워드·토픽 평가(관련 7점·영향 6점·입력 500토큰)
pub fn fake_rating() -> LeadRating {
    LeadRating {
        relevance: 7.0,
        relevance_confidence: 0.7,
        magnitude: 6.0,
        magnitude_confidence: 0.7,
        model: "jev-1.13.0".to_string(),
        usage: TokenUsage {
            input_tokens: 500,
            output_tokens: 20,
        },
    }
}

type RateAnswer = Box<dyn Fn(&LeadInput) -> Result<LeadRating, DeciderError> + Send + Sync>;
type MergedAnswer = Box<dyn Fn(&MergedInput) -> Result<MergedDecision, DeciderError> + Send + Sync>;
type DecideAnswer = Box<dyn Fn(&DecisionInput) -> Result<Decision, DeciderError> + Send + Sync>;

/// 기본 평가 결과(관련 7점·영향 7점·호재·입력 1000토큰)
pub fn fake_decision() -> Decision {
    Decision {
        relevance: 7.0,
        relevance_confidence: 0.7,
        magnitude: 7.0,
        magnitude_confidence: 0.7,
        impact: crate::server::db::schema::Impact::Positive,
        impact_confidence: 0.8,
        impact_probabilities: [0.8, 0.1, 0.1],
        bystander: Some(0.1),
        model: "jev-1.13.0".to_string(),
        usage: TokenUsage {
            input_tokens: 1000,
            output_tokens: 50,
        },
    }
}

/// 기본 합친 평가 결과: 토픽·시장 관련 7점, 증시 영향 7점·호재, 입력 1000토큰
pub fn fake_merged_decision(input: &MergedInput) -> MergedDecision {
    let scored = Scored {
        score: 7.0,
        confidence: 0.7,
    };
    MergedDecision {
        topics: input.topics.iter().map(|_| scored).collect(),
        markets: input
            .markets
            .iter()
            .map(|&market| MarketVerdict {
                market,
                relevance: input.market_items.contains(&market).then_some(scored),
                magnitude: scored,
                impact: crate::server::db::schema::Impact::Positive,
                impact_confidence: 0.8,
                impact_probabilities: [0.8, 0.1, 0.1],
            })
            .collect(),
        model: "jev-1.13.0".to_string(),
        usage: TokenUsage {
            input_tokens: 1000,
            output_tokens: 50,
        },
    }
}

/// 입력마다 정한 결과(또는 오류)를 돌려주는 가짜 Jev. 받은 입력과 모델을 `calls`(기사 평가)와
/// `rate_calls`(키워드·토픽 평가)에 남긴다.
pub struct FakeDecider {
    decide_answer: DecideAnswer,
    rate_answer: RateAnswer,
    merged_answer: MergedAnswer,
    pub calls: Mutex<Vec<(DecisionInput, String)>>,
    /// 합친 평가(기사당 한 요청) 호출
    pub merged_calls: Mutex<Vec<(MergedInput, String)>>,
    pub rate_calls: Mutex<Vec<(LeadInput, String)>>,
}

/// 키워드·토픽 평가만 정한다(기사 평가는 기본 결과).
pub fn fake_decider(
    rate_answer: impl Fn(&LeadInput) -> Result<LeadRating, DeciderError> + Send + Sync + 'static,
) -> FakeDecider {
    FakeDecider {
        decide_answer: Box::new(|_| Ok(fake_decision())),
        rate_answer: Box::new(rate_answer),
        merged_answer: Box::new(|input| Ok(fake_merged_decision(input))),
        merged_calls: Mutex::new(Vec::new()),
        calls: Mutex::new(Vec::new()),
        rate_calls: Mutex::new(Vec::new()),
    }
}

/// 기사 평가만 정한다(키워드·토픽 평가는 기본 결과).
pub fn fake_article_decider(
    decide_answer: impl Fn(&DecisionInput) -> Result<Decision, DeciderError> + Send + Sync + 'static,
) -> FakeDecider {
    FakeDecider {
        decide_answer: Box::new(decide_answer),
        rate_answer: Box::new(|_| Ok(fake_rating())),
        merged_answer: Box::new(|input| Ok(fake_merged_decision(input))),
        merged_calls: Mutex::new(Vec::new()),
        calls: Mutex::new(Vec::new()),
        rate_calls: Mutex::new(Vec::new()),
    }
}

impl FakeDecider {
    /// 합친 평가의 결과(또는 오류)를 정한다.
    pub fn with_merged(
        mut self,
        answer: impl Fn(&MergedInput) -> Result<MergedDecision, DeciderError> + Send + Sync + 'static,
    ) -> Self {
        self.merged_answer = Box::new(answer);
        self
    }
}

impl crate::server::ai::decider::types::Decider for FakeDecider {
    async fn decide(&self, input: &DecisionInput, model: &str) -> Result<Decision, DeciderError> {
        self.calls
            .lock()
            .unwrap()
            .push((input.clone(), model.to_string()));
        (self.decide_answer)(input)
    }

    async fn decide_merged(
        &self,
        input: &MergedInput,
        model: &str,
    ) -> Result<MergedDecision, DeciderError> {
        self.merged_calls
            .lock()
            .unwrap()
            .push((input.clone(), model.to_string()));
        (self.merged_answer)(input)
    }

    async fn rate(&self, input: &LeadInput, model: &str) -> Result<LeadRating, DeciderError> {
        self.rate_calls
            .lock()
            .unwrap()
            .push((input.clone(), model.to_string()));
        (self.rate_answer)(input)
    }
}

/// `insert_link`에 주는 값(`tests/fixtures.ts`의 `insertLink` 인자)
pub struct LinkSpec<'a> {
    pub stock_id: i64,
    pub title: Option<&'a str>,
    pub published_at: crate::server::time::EpochMs,
    pub kind: &'a str,
    /// 주면 그 점수로 평가한 것으로 한다
    pub relevance: Option<f64>,
    /// 영향도(11단계). 주지 않으면 11단계 전 평가처럼 비운다
    pub magnitude: Option<f64>,
}

impl<'a> LinkSpec<'a> {
    pub fn new(stock_id: i64, title: &'a str, published_at: crate::server::time::EpochMs) -> Self {
        Self {
            stock_id,
            title: Some(title),
            published_at,
            kind: "name",
            relevance: Some(7.0),
            magnitude: None,
        }
    }

    pub fn with_relevance(mut self, relevance: f64) -> Self {
        self.relevance = Some(relevance);
        self
    }
}

static LINK_SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 기사 하나와 종목 연결 하나를 넣는다. `relevance`가 있으면 평가도 넣는다.
pub fn insert_link(conn: &Connection, link: &LinkSpec<'_>) -> (i64, i64) {
    let serial = LINK_SERIAL.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    conn.execute(
        "INSERT INTO articles (url_key, url, title, summary, publisher, feed, published_at, collected_at) \
         VALUES (?1, ?2, ?3, NULL, '가경제', 'rss:a:economy', ?4, ?4)",
        rusqlite::params![
            format!("fixture.example/{serial}"),
            format!("https://fixture.example/{serial}"),
            link.title.map_or_else(|| format!("기사 {serial}"), str::to_string),
            link.published_at
        ],
    )
    .unwrap();
    let article_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO article_matches (article_id, stock_id, keyword_path, keyword_reason, match_kind, level, created_at) \
         VALUES (?1, ?2, '종목', NULL, ?3, ?4, ?5)",
        rusqlite::params![article_id, link.stock_id, link.kind, i64::from(link.kind == "keyword"), link.published_at],
    )
    .unwrap();
    let match_id = conn.last_insert_rowid();
    if let Some(relevance) = link.relevance {
        conn.execute(
            "INSERT INTO evaluations (match_id, article_id, stock_id, relevance, relevance_confidence, magnitude, \
             magnitude_confidence, impact, impact_confidence, impact_probabilities, model, evaluated_at) \
             VALUES (?1, ?2, ?3, ?4, 0.7, ?6, ?7, 'neutral', 0.7, \
             '{\"positive\":0.1,\"negative\":0.1,\"neutral\":0.8}', 'jev-1.13.0', ?5)",
            rusqlite::params![
                match_id,
                article_id,
                link.stock_id,
                relevance,
                link.published_at,
                link.magnitude,
                link.magnitude.map(|_| 0.7)
            ],
        )
        .unwrap();
    }
    (match_id, article_id)
}

// ---- 시장·주제 항목 씨앗: R7의 `topics/store` 함수가 만드는 것과 같은 모양의 행을 SQL로 직접 넣는다. ----

/// 시장 항목 한 줄(`ensureDefaultMarket`·`createMarketRow`와 같은 값)
pub fn seed_market(conn: &Connection, market: &str, now: crate::server::time::EpochMs) -> i64 {
    let (name, exchange) = if market == "KR" {
        ("한국 증시", "KOSPI·KOSDAQ")
    } else {
        ("미국 증시", "S&P 500·NASDAQ")
    };
    conn.execute(
        "INSERT INTO stocks (market, code, name, name_ko, exchange, user_input, kind, keywords_status, backfill_from, created_at) \
         VALUES (?1, 'MARKET', ?2, ?2, ?3, ?2, 'market', 'pending', ?4, ?5)",
        rusqlite::params![market, name, exchange, now - 24 * 60 * 60 * 1000, now],
    )
    .unwrap();
    conn.last_insert_rowid()
}

/// 토픽 한 줄. `terms`는 JSON 목록으로 저장한다.
pub fn seed_topic(
    conn: &Connection,
    source: &str,
    name: &str,
    terms: &[&str],
    now: crate::server::time::EpochMs,
) -> i64 {
    conn.execute(
        "INSERT INTO topics (source, name, terms, created_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![source, name, serde_json::to_string(terms).unwrap(), now],
    )
    .unwrap();
    conn.last_insert_rowid()
}

/// 토픽의 증시 기준 항목(`syncTopicLenses`가 만드는 줄). 자동 토픽은 키워드 상태가 이미 'ready'다.
pub fn seed_lens(
    conn: &Connection,
    topic_id: i64,
    name: &str,
    source: &str,
    market: &str,
    now: crate::server::time::EpochMs,
) -> i64 {
    let exchange = if market == "KR" {
        "한국 증시"
    } else {
        "미국 증시"
    };
    let auto = source == "auto";
    conn.execute(
        "INSERT INTO stocks (market, code, name, name_ko, exchange, user_input, kind, topic_id, keywords_status, \
         keywords_generated_at, backfill_from, created_at) VALUES (?1, ?2, ?3, ?3, ?4, ?3, 'topic', ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            market,
            format!("TOPIC-{topic_id}"),
            name,
            exchange,
            topic_id,
            if auto { "ready" } else { "pending" },
            if auto { Some(now) } else { None },
            if auto { None } else { Some(now - 24 * 60 * 60 * 1000) },
            now
        ],
    )
    .unwrap();
    conn.last_insert_rowid()
}

/// 토픽 갱신이 종목에 붙이는 가지(`attachTopicToStock`): 1단계 토픽 이름, 2단계 검색어. topic_id가 있고 종류는 시사(current)다.
pub fn attach_topic(
    conn: &Connection,
    stock_id: i64,
    stock_display: &str,
    topic_id: i64,
    topic_name: &str,
    terms: &[&str],
    now: crate::server::time::EpochMs,
) {
    let theme_path = format!("{stock_display} → {topic_name}");
    conn.execute(
        "INSERT INTO keywords (stock_id, level, text_ko, reason, path, kind, relevance, magnitude, topic_id, created_at) \
         VALUES (?1, 1, ?2, '수출 규제 영향', ?3, 'current', 7, 6, ?4, ?5)",
        rusqlite::params![stock_id, topic_name, theme_path, topic_id, now],
    )
    .unwrap();
    let theme_id = conn.last_insert_rowid();
    for term in terms {
        conn.execute(
            "INSERT INTO keywords (stock_id, level, parent_id, text_ko, reason, path, kind, topic_id, created_at) \
             VALUES (?1, 2, ?2, ?3, '토픽 검색어', ?4, 'current', ?5, ?6)",
            rusqlite::params![stock_id, theme_id, term, format!("{theme_path} → {term}"), topic_id, now],
        )
        .unwrap();
    }
}

/// `insert_link`가 만든 연결의 키워드 경로와 걸러짐(`gated`)을 바꾼다(키워드 실적·증분 점검 시험용).
pub fn set_link_path(conn: &Connection, match_id: i64, keyword_path: &str, gated: bool) {
    conn.execute(
        "UPDATE article_matches SET keyword_path = ?1, gated = ?2 WHERE id = ?3",
        rusqlite::params![keyword_path, gated, match_id],
    )
    .unwrap();
}
