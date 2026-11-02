//! web에서 쓰는 읽기 전용 DB(`db/web.ts`). 처음 부를 때 열고, 성공한 연결만 보관한다.
//! Playwright가 DB를 새로 만드는 중에 web이 먼저 뜰 수 있어, 시작할 때 열지 않고 요청 때 연다.
use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;

use super::client::{DbAccess, open_db};

pub struct WebDb {
    /// `DATABASE_PATH`(없으면 그 사실을 오류 글자로 보관)
    path: Result<PathBuf, String>,
    conn: Mutex<Option<Connection>>,
}

impl WebDb {
    pub fn new(path: Result<PathBuf, String>) -> Self {
        Self {
            path,
            conn: Mutex::new(None),
        }
    }

    /// 연결로 `f`를 실행한다. 아직 못 열었으면 열어 본다. 오류는 화면에 보여 줄 글자로 돌려준다.
    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T, String> {
        let mut guard = self
            .conn
            .lock()
            .map_err(|_| "DB 잠금이 깨졌습니다.".to_string())?;
        if guard.is_none() {
            let path = self.path.as_ref().map_err(Clone::clone)?;
            *guard = Some(open_db(path, DbAccess::ReadOnly).map_err(|e| e.to_string())?);
        }
        let conn = guard.as_ref().expect("방금 열었다");
        f(conn).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
#[path = "web_tests.rs"]
mod tests;
