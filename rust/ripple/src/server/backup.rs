use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use rusqlite::Connection;
use rusqlite::backup::Backup;

use crate::server::db::client::{DbAccess, open_db};
use crate::server::settings::{BACKUP_LAST_AT, read_num};
use crate::server::time::{
    EpochMs, format_seoul_date_time, last_seoul_boundary, next_seoul_boundary,
};

/// 자동 백업 시각(한국 시간)과 남길 개수 (D-27, 2026-10-21 결정)
pub const BACKUP_HOUR: i64 = 4;
pub const BACKUP_KEEP: usize = 7;

static BACKUP_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^ripple-\d{8}-\d{6}\.db$").expect("정규식이 올바르다"));

/// "ripple-20261022-040003.db"(한국 시간). 이름 순서가 시간 순서다.
pub fn backup_file_name(at: EpochMs) -> String {
    let text = format_seoul_date_time(at); // YYYY-MM-DD HH:mm:ss
    format!(
        "ripple-{}-{}.db",
        text[..10].replace('-', ""),
        text[11..].replace(':', "")
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupPlan {
    pub due: bool,
    pub next_at: EpochMs,
}

/// 오늘 04:00(한국 시간) 뒤에 아직 백업하지 않았으면 지금. 그 시각에 꺼져 있었으면 켜질 때 한 번.
pub fn backup_plan(conn: &Connection, now: EpochMs) -> rusqlite::Result<BackupPlan> {
    let boundary = last_seoul_boundary(now, BACKUP_HOUR, false);
    let due = read_num(conn, &BACKUP_LAST_AT)? < boundary as f64;
    Ok(BackupPlan {
        due,
        next_at: if due {
            now
        } else {
            next_seoul_boundary(now, BACKUP_HOUR, false)
        },
    })
}

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("백업 작업이 중단됐습니다: {0}")]
    Join(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupResult {
    pub file: String,
    pub bytes: u64,
    pub removed: Vec<String>,
}

/// 한 번에 옮기는 쪽 수와 쪽 사이 쉬는 시간(쓰는 쪽이 끼어들 틈을 준다)
const BACKUP_PAGES_PER_STEP: std::os::raw::c_int = 256;
const BACKUP_PAUSE: Duration = Duration::from_millis(0);

/// SQLite 온라인 백업(D-27). 쓰는 중에도 일관된 사본을 만들고, WAL 없이 파일 하나로 열리게
/// 저널 모드를 DELETE로 바꿔 둔다(복원할 때 그대로 복사하면 된다). 최근 7개만 남긴다.
/// 잠금을 오래 쥐지 않게(설계 3.6) 공유 연결 대신 DB 파일을 읽기 전용으로 따로 연다. 오래 걸리므로
/// 비동기 작업에서는 `run_backup`(spawn_blocking)으로 부른다.
pub fn backup_database(
    database_path: &Path,
    dir: &Path,
    now: EpochMs,
) -> Result<BackupResult, BackupError> {
    std::fs::create_dir_all(dir)?;
    let file = backup_file_name(now);
    let target = dir.join(&file);
    {
        let source = open_db(database_path, DbAccess::ReadOnly)?;
        let mut copy = Connection::open(&target)?;
        Backup::new(&source, &mut copy)?.run_to_completion(
            BACKUP_PAGES_PER_STEP,
            BACKUP_PAUSE,
            None,
        )?;
        // 결과 한 줄을 돌려주는 PRAGMA라 query_row로 읽는다.
        let _mode: String = copy.query_row("PRAGMA journal_mode = DELETE", [], |row| row.get(0))?;
        copy.close().map_err(|(_, error)| error)?;
    }
    let bytes = std::fs::metadata(&target)?.len();

    let mut names: Vec<String> = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| BACKUP_NAME.is_match(name))
        .collect();
    // JS 기본 정렬(UTF-16 순서)과 같게. 이름이 모두 ASCII라 바이트 순서와 같다.
    names.sort();
    names.reverse();
    let removed: Vec<String> = names.into_iter().skip(BACKUP_KEEP).collect();
    for name in &removed {
        for suffix in ["", "-wal", "-shm", "-journal"] {
            match std::fs::remove_file(dir.join(format!("{name}{suffix}"))) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(BackupResult {
        file,
        bytes,
        removed,
    })
}

/// 별도 스레드에서 백업한다(스케줄러가 부름).
pub async fn run_backup(
    database_path: PathBuf,
    dir: PathBuf,
    now: EpochMs,
) -> Result<BackupResult, BackupError> {
    tokio::task::spawn_blocking(move || backup_database(&database_path, &dir, now))
        .await
        .map_err(|error| BackupError::Join(error.to_string()))?
}

#[cfg(test)]
#[path = "backup_tests.rs"]
mod tests;
