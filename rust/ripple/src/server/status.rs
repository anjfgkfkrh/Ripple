//! 0단계 확인 화면용 시스템 상태(`status.ts`): web에서 DB(읽기 전용)와 worker 내부 API에 닿는지 확인한다.
use rusqlite::Connection;

use crate::server::db::client::read_journal_mode;
use crate::server::db::web::WebDb;
use crate::server::time::{EpochMs, now_ms};
use crate::server::worker_client::{WorkerHealth, fetch_worker_health};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatabaseStatus {
    Ok {
        journal_mode: String,
        settings_count: i64,
    },
    Err {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SystemStatus {
    pub database: DatabaseStatus,
    pub worker: WorkerHealth,
    pub checked_at: EpochMs,
}

pub const WORKER_HEALTH_TIMEOUT_MS: u64 = 2000;

pub async fn get_system_status(
    db: &WebDb,
    worker_url: Option<&str>,
    http: &reqwest::Client,
) -> SystemStatus {
    let database = match db.with(check_database) {
        Ok(status) => status,
        Err(reason) => DatabaseStatus::Err { reason },
    };
    let worker = match worker_url {
        Some(url) if !url.is_empty() => {
            fetch_worker_health(http, url, WORKER_HEALTH_TIMEOUT_MS).await
        }
        _ => WorkerHealth::Err {
            reason: "환경변수 WORKER_URL이 설정되지 않았습니다.".to_string(),
        },
    };
    SystemStatus {
        database,
        worker,
        checked_at: now_ms(),
    }
}

fn check_database(conn: &Connection) -> rusqlite::Result<DatabaseStatus> {
    Ok(DatabaseStatus::Ok {
        journal_mode: read_journal_mode(conn)?,
        settings_count: conn.query_row("SELECT count(*) FROM settings", [], |r| r.get(0))?,
    })
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
