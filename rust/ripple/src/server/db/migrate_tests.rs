use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use rusqlite::Connection;
use tempfile::TempDir;

use super::*;
use crate::server::db::client::{DbAccess, enable_wal, open_db};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn columns_of(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    stmt.query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn user_version(conn: &Connection) -> i64 {
    conn.query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}

/// TS(drizzle)로 DB를 만든다: `pnpm exec tsx src/worker/migrate.ts`
fn apply_with_drizzle(db_path: &Path) {
    let output = Command::new("pnpm")
        .args(["exec", "tsx", "src/worker/migrate.ts"])
        .current_dir(repo_root())
        .env("DATABASE_PATH", db_path)
        .output()
        .expect("pnpm을 실행할 수 없습니다");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

type SchemaRow = (String, String, String, Option<String>);

fn schema_dump(db_path: &Path) -> Vec<SchemaRow> {
    let conn = Connection::open(db_path).unwrap();
    let mut stmt = conn
        .prepare("SELECT type, name, tbl_name, sql FROM sqlite_master WHERE tbl_name NOT LIKE '\\_\\_drizzle%' ESCAPE '\\' ORDER BY name")
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

// describe("applyMigrations")
#[test]
fn creates_db_file_and_parent_folder_with_settings_table() {
    // it("DB 파일과 상위 폴더가 없으면 만들고 settings 테이블을 만든다")
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("nested").join("ripple.db");
    apply_migrations(&db_path).unwrap();

    let db = open_db(&db_path, DbAccess::ReadOnly).unwrap();
    assert_eq!(columns_of(&db, "settings"), ["key", "value", "updated_at"]);
    assert_eq!(user_version(&db), 11);
}

#[test]
fn rerun_skips_already_applied() {
    // it("다시 실행해도 이미 적용한 것은 건너뛴다")
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("ripple.db");
    apply_migrations(&db_path).unwrap();
    apply_migrations(&db_path).unwrap();
}

#[test]
fn applies_new_migration_while_web_and_worker_connections_are_open() {
    // it("web(읽기 전용)·worker(쓰기) 연결이 열려 있어도 새 마이그레이션을 적용한다")
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("ripple.db");
    apply_migrations(&db_path).unwrap();
    let worker = open_db(&db_path, DbAccess::ReadWrite).unwrap();
    enable_wal(&worker).unwrap();
    let web = open_db(&db_path, DbAccess::ReadOnly).unwrap();
    // 두 연결 모두 실제로 한 번 읽어 둔 상태로 둔다.
    worker
        .query_row("SELECT count(*) FROM settings", [], |r| r.get::<_, i64>(0))
        .unwrap();
    web.query_row("SELECT count(*) FROM settings", [], |r| r.get::<_, i64>(0))
        .unwrap();

    let mut sql = MIGRATION_SQL.to_vec();
    sql.push("ALTER TABLE `settings` ADD `extra` text;");
    let started = Instant::now();
    apply_migration_set(&db_path, &sql).unwrap();

    // 잠금 대기(5초) 없이 바로 끝나야 한다.
    assert!(started.elapsed().as_millis() < 1000);
    assert!(columns_of(&web, "settings").contains(&"extra".to_string()));
}

#[test]
fn migration_0008_fixes_future_published_times() {
    // it("0008: 이미 저장한 미래 발행 시각(받은 시각 + 10분 초과)을 받은 시각으로 고친다")
    // 0007까지만 적용한 DB에 미래 시각 기사·제목을 넣고 나머지를 적용한다.
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("ripple.db");
    apply_migration_set(&db_path, &MIGRATION_SQL[..8]).unwrap();

    let seen: i64 = 1_792_000_000_000;
    let raw = Connection::open(&db_path).unwrap();
    let insert = |url: &str, published: i64| {
        raw.execute(
            "insert into articles (url_key, url, title, publisher, feed, published_at, collected_at) values (?1, ?1, 't', 'p', 'f', ?2, ?3)",
            rusqlite::params![url, published, seen],
        )
        .unwrap();
    };
    insert("a.example/future", seen + 19 * 3_600_000);
    insert("a.example/close", seen + 5 * 60_000);
    insert("a.example/past", seen - 3_600_000);
    raw.execute(
        "insert into feed_headlines (title_key, title, publisher, feed, published_at, seen_at) values ('k', 't', 'p', 'f', ?1, ?2)",
        rusqlite::params![seen + 5 * 3_600_000, seen],
    )
    .unwrap();
    drop(raw);

    apply_migrations(&db_path).unwrap();
    let db = open_db(&db_path, DbAccess::ReadOnly).unwrap();
    let mut stmt = db
        .prepare("select url_key, published_at from articles order by id")
        .unwrap();
    let rows: Vec<(String, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        rows,
        [
            ("a.example/future".to_string(), seen),
            ("a.example/close".to_string(), seen + 5 * 60_000),
            ("a.example/past".to_string(), seen - 3_600_000),
        ]
    );
    let headline: i64 = db
        .query_row("select published_at from feed_headlines", [], |r| r.get(0))
        .unwrap();
    assert_eq!(headline, seen);
}

// 설계 3.4의 테스트 3개와 복사본 확인
#[test]
fn migration_sql_files_are_identical_to_drizzle_folder() {
    // 설계 3.4: `drizzle/*.sql`을 내용 그대로 복사했다.
    let mut names: Vec<_> = std::fs::read_dir(repo_root().join("drizzle"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".sql"))
        .collect();
    names.sort();
    assert_eq!(names.len(), MIGRATION_SQL.len());
    for (name, sql) in names.iter().zip(MIGRATION_SQL) {
        let original = std::fs::read_to_string(repo_root().join("drizzle").join(name)).unwrap();
        assert_eq!(original, sql, "{name}");
    }
}

#[test]
fn drizzle_baseline_constants_match_journal() {
    let journal = std::fs::read_to_string(repo_root().join("drizzle/meta/_journal.json")).unwrap();
    let journal: serde_json::Value = serde_json::from_str(&journal).unwrap();
    let entries = journal["entries"].as_array().unwrap();
    assert_eq!(entries.len() as i64, DRIZZLE_APPLIED_COUNT);
    assert_eq!(
        entries.last().unwrap()["when"].as_i64().unwrap(),
        DRIZZLE_LAST_CREATED_AT
    );
}

#[test]
fn schema_applied_by_rust_equals_schema_applied_by_drizzle() {
    // 설계 3.4 ①: 새 파일 DB에 Rust로 적용한 결과와 TS로 적용한 결과의 sqlite_master가 같다(`__drizzle_migrations` 줄 제외).
    let dir = TempDir::new().unwrap();
    let rust_db = dir.path().join("rust.db");
    let ts_db = dir.path().join("ts.db");
    apply_migrations(&rust_db).unwrap();
    apply_with_drizzle(&ts_db);
    assert_eq!(schema_dump(&rust_db), schema_dump(&ts_db));
}

#[test]
fn drizzle_db_gets_baseline_once_and_schema_is_unchanged() {
    // 설계 3.4 ②: drizzle로 적용한 DB에 migrate를 두 번 돌려도 스키마가 그대로이고 user_version이 11이다.
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("ts.db");
    apply_with_drizzle(&db_path);
    let before = schema_dump(&db_path);
    assert_eq!(user_version(&Connection::open(&db_path).unwrap()), 0);

    apply_migrations(&db_path).unwrap();
    apply_migrations(&db_path).unwrap();

    assert_eq!(schema_dump(&db_path), before);
    let conn = Connection::open(&db_path).unwrap();
    assert_eq!(user_version(&conn), 11);
    let rows: i64 = conn
        .query_row("SELECT count(*) FROM __drizzle_migrations", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(rows, 11, "__drizzle_migrations는 지우지 않는다");
}

#[test]
fn drizzle_db_with_unexpected_history_fails_without_changes() {
    // 설계 3.4 ③: `__drizzle_migrations` 줄 수가 10인 DB에서는 실패하고 아무것도 바뀌지 않는다.
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("ts.db");
    apply_with_drizzle(&db_path);
    Connection::open(&db_path)
        .unwrap()
        .execute(
            "DELETE FROM __drizzle_migrations WHERE rowid = (SELECT max(rowid) FROM __drizzle_migrations)",
            [],
        )
        .unwrap();
    let before = schema_dump(&db_path);

    let err = apply_migrations(&db_path).unwrap_err();
    assert!(
        matches!(
            err,
            MigrateError::UnexpectedDrizzleHistory { count: 10, .. }
        ),
        "{err}"
    );
    assert!(err.to_string().contains("줄 수 10"));
    assert_eq!(schema_dump(&db_path), before);
    assert_eq!(user_version(&Connection::open(&db_path).unwrap()), 0);
}
