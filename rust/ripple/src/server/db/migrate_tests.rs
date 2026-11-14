use std::path::{Path, PathBuf};
use std::time::Instant;

use rusqlite::Connection;
use tempfile::TempDir;

use super::*;
use crate::server::db::client::{DbAccess, enable_wal, open_db};
use crate::server::evaluate::run::NOT_HIDDEN_SQL;

/// TS(drizzle) 시절의 고정 파일: `drizzle/`(마이그레이션 원본과 journal)와 TS로 만든 DB의 `.dump`(`applied.sql`).
fn drizzle_fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/drizzle")
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

/// TS(drizzle)로 적용한 DB를 그대로 만든다: TS 제거 전에 `sqlite3 .dump`로 떠 둔 `applied.sql`을 실행한다.
fn apply_with_drizzle(db_path: &Path) {
    let sql = std::fs::read_to_string(drizzle_fixtures().join("applied.sql")).unwrap();
    Connection::open(db_path)
        .unwrap()
        .execute_batch(&sql)
        .unwrap();
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
    assert_eq!(user_version(&db), MIGRATION_SQL.len() as i64);
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
    // 설계 3.4: `drizzle/*.sql`을 내용 그대로 복사했다(고정 복사본과 같은지 본다).
    let mut names: Vec<_> = std::fs::read_dir(drizzle_fixtures())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".sql") && n != "applied.sql")
        .collect();
    names.sort();
    // 0000~0010만 drizzle 복사본이다. 0011부터는 Rust 전용이라 따로 시험한다.
    assert_eq!(names.len() as i64, DRIZZLE_APPLIED_COUNT);
    for (name, sql) in names
        .iter()
        .zip(&MIGRATION_SQL[..DRIZZLE_APPLIED_COUNT as usize])
    {
        let original = std::fs::read_to_string(drizzle_fixtures().join(name)).unwrap();
        assert_eq!(original, *sql, "{name}");
    }
}

#[test]
fn drizzle_baseline_constants_match_journal() {
    let journal = std::fs::read_to_string(drizzle_fixtures().join("meta/_journal.json")).unwrap();
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
    apply_migration_set(&rust_db, &MIGRATION_SQL[..11]).unwrap();
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

    apply_migration_set(&db_path, &MIGRATION_SQL[..11]).unwrap();
    apply_migration_set(&db_path, &MIGRATION_SQL[..11]).unwrap();

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

#[test]
fn drizzle_db_gets_rust_only_migrations_after_baseline() {
    // drizzle로 적용한 DB에 전체를 적용하면 user_version이 전체 길이가 되고, 스키마는 새 파일 DB와 같다.
    let dir = TempDir::new().unwrap();
    let ts_db = dir.path().join("ts.db");
    let fresh = dir.path().join("fresh.db");
    apply_with_drizzle(&ts_db);
    apply_migrations(&ts_db).unwrap();
    apply_migrations(&fresh).unwrap();

    assert_eq!(schema_dump(&ts_db), schema_dump(&fresh));
    let conn = Connection::open(&ts_db).unwrap();
    assert_eq!(user_version(&conn), 14);
    let rows: i64 = conn
        .query_row("SELECT count(*) FROM __drizzle_migrations", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(rows, 11);
}

#[test]
fn migration_0011_adds_anchor_columns_and_resets_stock_keywords() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("ripple.db");
    apply_migration_set(&db_path, &MIGRATION_SQL[..11]).unwrap();
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "INSERT INTO stocks (id, market, code, name, exchange, user_input, kind, keywords_status, keywords_error, created_at, deleted_at) VALUES
           (1,'KR','005930','삼성전자','KOSPI','삼성전자','stock','done','x',1,NULL),
           (2,'KR','000660','SK하이닉스','KOSPI','x','stock','done',NULL,1,1),
           (3,'KR','KOSPI','코스피','KOSPI','x','index','done',NULL,1,NULL);",
    )
    .unwrap();
    drop(conn);
    apply_migrations(&db_path).unwrap();

    let conn = Connection::open(&db_path).unwrap();
    assert_eq!(user_version(&conn), 14);
    let status = |id: i64| -> (String, Option<String>) {
        conn.query_row(
            "SELECT keywords_status, keywords_error FROM stocks WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    };
    assert_eq!(status(1), ("pending".to_string(), None));
    assert_eq!(status(2).0, "done");
    assert_eq!(status(3).0, "done");
    let (al, an, rm): (String, String, String) = conn
        .query_row(
            "SELECT aliases, anchors, aliases_removed FROM stocks WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((al.as_str(), an.as_str(), rm.as_str()), ("[]", "[]", "[]"));
    assert!(columns_of(&conn, "topics").contains(&"scope".to_string()));
    assert!(columns_of(&conn, "article_matches").contains(&"gated".to_string()));
}

#[test]
fn migration_0012_adds_nature_columns_with_null_for_existing_rows() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("ripple.db");
    apply_migration_set(&db_path, &MIGRATION_SQL[..12]).unwrap();
    let conn = Connection::open(&db_path).unwrap();
    let article_cols = columns_of(&conn, "articles");
    let eval_cols = columns_of(&conn, "evaluations");
    assert!(!article_cols.contains(&"nature_fact".to_string()));
    assert!(!eval_cols.contains(&"bystander".to_string()));
    drop(conn);
    apply_migrations(&db_path).unwrap();

    let conn = Connection::open(&db_path).unwrap();
    assert_eq!(user_version(&conn), 14);
    let article_cols = columns_of(&conn, "articles");
    for c in [
        "nature_fact",
        "nature_analysis",
        "nature_forecast",
        "nature_opinion",
        "nature_media",
        "nature_roundup",
        "nature_at",
    ] {
        assert!(article_cols.contains(&c.to_string()), "{c}");
    }
    assert!(columns_of(&conn, "evaluations").contains(&"bystander".to_string()));
}

#[test]
fn migration_0013_indexes_evaluations_by_article_for_hidden_rule() {
    // 숨김 규칙(`NOT_HIDDEN_SQL`)의 하위 쿼리가 같은 기사의 평가를 표 전체를 훑지 않고 색인으로 찾는다.
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("ripple.db");
    apply_migrations(&db_path).unwrap();
    let conn = Connection::open(&db_path).unwrap();
    assert_eq!(user_version(&conn), 14);
    let sql = format!(
        "EXPLAIN QUERY PLAN SELECT evaluations.id FROM evaluations \
         INNER JOIN articles ON articles.id = evaluations.article_id \
         INNER JOIN stocks ON stocks.id = evaluations.stock_id WHERE {NOT_HIDDEN_SQL}"
    );
    let plan: Vec<String> = conn
        .prepare(&sql)
        .unwrap()
        .query_map([0.5], |r| r.get(3))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert!(
        plan.iter()
            .any(|d| d.contains("SEARCH m USING INDEX evaluations_article")),
        "{plan:?}"
    );
    assert!(!plan.iter().any(|d| d == "SCAN m"), "{plan:?}");
}
