use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

/// DB 쓰기는 worker만 한다. web은 읽기 전용으로 연다. (SPEC D-36)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbAccess {
    ReadOnly,
    ReadWrite,
}

/// 연결 하나를 나눠 쓴다. 잠금은 짧게 잡고 `.await`를 쥔 채로 넘기지 않는다(설계 3.3).
pub type Db = Arc<Mutex<Connection>>;

/// 다른 연결이 잠금을 잡고 있을 때 기다리는 시간(SQLite busy_timeout)
const BUSY_TIMEOUT: Duration = Duration::from_millis(5000);

pub fn open_db(database_path: &Path, access: DbAccess) -> rusqlite::Result<Connection> {
    // DB 파일은 마이그레이션(migrate 서비스)이 만든다. 없으면 새로 만들지 않고 실패한다(CREATE 플래그 없음).
    let flags = match access {
        DbAccess::ReadOnly => OpenFlags::SQLITE_OPEN_READ_ONLY,
        DbAccess::ReadWrite => OpenFlags::SQLITE_OPEN_READ_WRITE,
    } | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let file = std::path::absolute(database_path).unwrap_or_else(|_| database_path.to_path_buf());
    let conn = Connection::open_with_flags(file, flags)?;
    conn.busy_timeout(BUSY_TIMEOUT)?;
    // better-sqlite3는 기본으로 켜져 있지만 rusqlite(bundled)는 꺼져 있다. 빼먹으면 지울 때·넣을 때 동작이 달라진다.
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(conn)
}

/// 열린 연결을 `Db`로 감싼다.
pub fn into_db(conn: Connection) -> Db {
    Arc::new(Mutex::new(conn))
}

/// 여러 문장을 한 덩어리로 실행한다(TS `db.transaction(() => ...)`). 이미 트랜잭션 안이면 savepoint가 되어
/// 바깥 것과 함께 묶이고(better-sqlite3와 같은 동작), 아니면 새 트랜잭션이 된다. 오류가 나면 되돌린다.
pub fn atomically<T>(
    conn: &Connection,
    body: impl FnOnce() -> rusqlite::Result<T>,
) -> rusqlite::Result<T> {
    conn.execute_batch("SAVEPOINT ripple_atomic")?;
    match body() {
        Ok(value) => {
            conn.execute_batch("RELEASE ripple_atomic")?;
            Ok(value)
        }
        Err(error) => {
            // 되돌리기에 실패해도 처음 오류를 돌려준다.
            let _ = conn.execute_batch("ROLLBACK TO ripple_atomic; RELEASE ripple_atomic");
            Err(error)
        }
    }
}

/// WAL 모드를 켠다. WAL은 DB 파일에 저장되는 설정이라 쓰는 쪽(worker)이 시작할 때 한 번 하면 된다.
/// WAL에서는 읽기와 쓰기가 서로 막지 않는다.
pub fn enable_wal(conn: &Connection) -> Result<(), String> {
    let mode: String = conn
        .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
        .map_err(|e| format!("WAL 모드를 켜지 못했습니다({e})."))?;
    if mode != "wal" {
        return Err(format!("WAL 모드를 켜지 못했습니다(현재: {mode})."));
    }
    Ok(())
}

pub fn read_journal_mode(conn: &Connection) -> rusqlite::Result<String> {
    conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
