use super::*;
use crate::server::settings::{LLM_MODEL, write_str};
use crate::test_support::test_db_file;

fn count_settings(conn: &Connection) -> i64 {
    conn.query_row("SELECT count(*) FROM settings", [], |r| r.get(0))
        .unwrap()
}

// describe("openDb")
#[test]
fn reads_and_writes_settings_table_made_by_migrations() {
    // it("마이그레이션으로 만든 settings 테이블을 읽고 쓴다")
    let file = test_db_file();
    let conn = open_db(&file.path, DbAccess::ReadWrite).unwrap();
    conn.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES ('test', '1', 1792000000000)",
        [],
    )
    .unwrap();
    let (value, updated_at): (String, i64) = conn
        .query_row(
            "SELECT value, updated_at FROM settings WHERE key = 'test'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(serde_json::from_str::<i64>(&value).unwrap(), 1);
    assert_eq!(updated_at, 1_792_000_000_000);
}

#[test]
fn updated_at_changes_when_value_is_updated() {
    // it("수정하면 updatedAt이 갱신된다") — drizzle의 $onUpdate는 SQL 밖 동작이라 Rust는 쓰기 함수가 now를 넣는다.
    let file = test_db_file();
    let conn = open_db(&file.path, DbAccess::ReadWrite).unwrap();
    write_str(&conn, &LLM_MODEL, "a", 1_000).unwrap();
    let before: i64 = conn
        .query_row("SELECT updated_at FROM settings", [], |r| r.get(0))
        .unwrap();
    write_str(&conn, &LLM_MODEL, "b", 1_005).unwrap();
    let after: i64 = conn
        .query_row("SELECT updated_at FROM settings", [], |r| r.get(0))
        .unwrap();
    assert!(after > before);
}

#[test]
fn fails_immediately_when_db_file_is_missing() {
    // it("DB 파일이 없으면 새로 만들지 않고 바로 실패한다")
    let file = test_db_file();
    let missing = file.dir.join("missing.db");
    assert!(open_db(&missing, DbAccess::ReadWrite).is_err());
    assert!(!missing.exists());
}

#[test]
fn foreign_keys_and_busy_timeout_are_set() {
    // Rust에서만 생기는 경우(설계 3.3): better-sqlite3는 기본으로 켜져 있어 rusqlite도 켜야 한다.
    let file = test_db_file();
    let conn = open_db(&file.path, DbAccess::ReadOnly).unwrap();
    let on: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    assert_eq!(on, 1);
    let timeout: i64 = conn
        .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
        .unwrap();
    assert_eq!(timeout, 5000);
}

// describe("web(읽기 전용) / worker(쓰기) 연결 (SPEC D-36)")
#[test]
fn wal_set_by_worker_is_visible_to_web_connection() {
    // it("worker가 WAL을 켜면 DB 파일에 남아 web 연결에서도 보인다")
    let file = test_db_file();
    let worker = open_db(&file.path, DbAccess::ReadWrite).unwrap();
    enable_wal(&worker).unwrap();
    let web = open_db(&file.path, DbAccess::ReadOnly).unwrap();
    assert_eq!(read_journal_mode(&web).unwrap(), "wal");
}

#[test]
fn read_only_connection_reads_but_rejects_writes() {
    // it("읽기 전용 연결은 읽기는 되고 쓰기는 거부된다")
    let file = test_db_file();
    let worker = open_db(&file.path, DbAccess::ReadWrite).unwrap();
    enable_wal(&worker).unwrap();
    worker
        .execute(
            "INSERT INTO settings (key, value, updated_at) VALUES ('a', '\"x\"', 0)",
            [],
        )
        .unwrap();

    let web = open_db(&file.path, DbAccess::ReadOnly).unwrap();
    assert_eq!(count_settings(&web), 1);
    let err = web
        .execute(
            "INSERT INTO settings (key, value, updated_at) VALUES ('b', '\"y\"', 0)",
            [],
        )
        .unwrap_err();
    assert!(err.to_string().to_lowercase().contains("readonly"), "{err}");
    assert_eq!(count_settings(&worker), 1);
}

#[test]
fn wal_read_is_not_blocked_by_write_transaction_and_hides_uncommitted_rows() {
    // it("WAL에서는 쓰기 트랜잭션 중에도 읽기가 막히지 않고, 커밋 전 내용은 보이지 않는다")
    let file = test_db_file();
    let mut worker = open_db(&file.path, DbAccess::ReadWrite).unwrap();
    enable_wal(&worker).unwrap();
    let web = open_db(&file.path, DbAccess::ReadOnly).unwrap();

    let tx = worker.transaction().unwrap();
    tx.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES ('pending', '1', 0)",
        [],
    )
    .unwrap();
    assert_eq!(count_settings(&web), 0);
    tx.commit().unwrap();

    assert_eq!(count_settings(&web), 1);
}
