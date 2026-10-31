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

/// 실제 KIND·NASDAQ 파일에서 고른 표본(KIND는 원본처럼 EUC-KR). 저장소 루트의 `tests/fixtures`를 그대로 읽는다.
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
        .join("../../tests/fixtures")
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
