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
