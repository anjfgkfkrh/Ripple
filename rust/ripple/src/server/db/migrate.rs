use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};
use rusqlite_migration::{M, Migrations};

/// `migrations/`의 SQL(0000~0010은 drizzle 마이그레이션 `drizzle/0000~0010`을 내용 그대로 복사한 것, 0011부터 Rust 전용). 순서 = 파일 이름 순서.
/// `--> statement-breakpoint`는 SQL 주석이라 그대로 실행된다.
pub const MIGRATION_SQL: [&str; 12] = [
    include_str!("../../../migrations/0000_init.sql"),
    include_str!("../../../migrations/0001_stocks_listings_usage.sql"),
    include_str!("../../../migrations/0002_keywords.sql"),
    include_str!("../../../migrations/0003_collection.sql"),
    include_str!("../../../migrations/0004_evaluations.sql"),
    include_str!("../../../migrations/0005_stock_backfill.sql"),
    include_str!("../../../migrations/0006_market_issues.sql"),
    include_str!("../../../migrations/0007_topics_scores.sql"),
    include_str!("../../../migrations/0008_fix_future_published.sql"),
    include_str!("../../../migrations/0009_read_articles.sql"),
    include_str!("../../../migrations/0010_stock_prices.sql"),
    include_str!("../../../migrations/0011_keyword_anchors.sql"),
];

/// drizzle이 적용해 둔 DB의 기준선: `__drizzle_migrations`의 줄 수와 마지막 `created_at`
/// (= `drizzle/meta/_journal.json`의 0010 `when`).
pub const DRIZZLE_APPLIED_COUNT: i64 = 11;
pub const DRIZZLE_LAST_CREATED_AT: i64 = 1_793_341_641_528;

#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    Migration(#[from] rusqlite_migration::Error),
    #[error(
        "drizzle 이력이 기대와 다릅니다(줄 수 {count}, 기대 {DRIZZLE_APPLIED_COUNT}; 마지막 created_at {last}, 기대 {DRIZZLE_LAST_CREATED_AT}). DB를 바꾸지 않았습니다."
    )]
    UnexpectedDrizzleHistory { count: i64, last: i64 },
}

/// 아직 적용하지 않은 마이그레이션을 적용한다. DB 파일이 없으면 만든다(DB 파일을 만드는 유일한 곳).
/// 배타 잠금을 걸지 않으므로 web·worker가 DB를 열고 있어도 적용된다.
pub fn apply_migrations(database_path: &Path) -> Result<(), MigrateError> {
    apply_migration_set(database_path, &MIGRATION_SQL)
}

/// `apply_migrations`와 같지만 SQL 목록을 받는다(테스트에서 일부만·덧붙여 적용할 때).
pub fn apply_migration_set(database_path: &Path, sql: &[&str]) -> Result<(), MigrateError> {
    let file = std::path::absolute(database_path)?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut conn = Connection::open_with_flags(
        &file,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(Duration::from_millis(5000))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    baseline_drizzle(&mut conn)?;
    let migrations = Migrations::new(sql.iter().map(|s| M::up(s)).collect());
    migrations.to_latest(&mut conn)?;
    Ok(())
}

/// drizzle이 적용한 DB(`user_version`이 0이고 `__drizzle_migrations` 표가 있음)는 기준선으로 `user_version`을 쓴다.
/// 줄 수나 값이 기대와 다르면 아무것도 바꾸지 않고 실패한다.
fn baseline_drizzle(conn: &mut Connection) -> Result<(), MigrateError> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version != 0 {
        return Ok(());
    }
    let has_table: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '__drizzle_migrations')",
        [],
        |row| row.get(0),
    )?;
    if !has_table {
        return Ok(());
    }
    let (count, last): (i64, Option<i64>) = conn.query_row(
        "SELECT count(*), max(created_at) FROM __drizzle_migrations",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let last = last.unwrap_or(0);
    if count != DRIZZLE_APPLIED_COUNT || last != DRIZZLE_LAST_CREATED_AT {
        return Err(MigrateError::UnexpectedDrizzleHistory { count, last });
    }
    conn.pragma_update(None, "user_version", DRIZZLE_APPLIED_COUNT)?;
    Ok(())
}

#[cfg(test)]
#[path = "migrate_tests.rs"]
mod tests;
