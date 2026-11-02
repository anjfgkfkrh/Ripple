use super::*;
use crate::server::db::client::enable_wal;
use crate::server::settings::{LLM_MODEL, write_num, write_str};
use crate::test_support::{ms, test_db};

// 한국 시간 2026-10-22(목) 04:00:03
fn now() -> EpochMs {
    ms("2026-10-21T19:00:03Z")
}

// describe("backupFileName")
#[test]
fn file_name_uses_korean_time_so_name_order_is_time_order() {
    // it("한국 시간으로 이름을 붙여 이름 순서가 시간 순서다")
    assert_eq!(backup_file_name(now()), "ripple-20261022-040003.db");
}

// describe("backupPlan (D-27: 매일 04:00)")
#[test]
fn due_after_four_when_not_yet_done_else_tomorrow_four() {
    // it("오늘 04:00 뒤에 아직 하지 않았으면 지금, 했으면 내일 04:00")
    let db = test_db();
    assert_eq!(
        backup_plan(&db.conn, now()).unwrap(),
        BackupPlan {
            due: true,
            next_at: now()
        }
    );
    write_num(&db.conn, &BACKUP_LAST_AT, now() as f64, now()).unwrap();
    assert_eq!(
        backup_plan(&db.conn, now()).unwrap(),
        BackupPlan {
            due: false,
            next_at: ms("2026-10-22T19:00:00Z")
        }
    );
    // 04:00 전(한국 03:59)이면 어제 04:00 뒤에 했는지만 본다.
    let before = ms("2026-10-21T18:59:00Z");
    write_num(
        &db.conn,
        &BACKUP_LAST_AT,
        ms("2026-10-20T19:30:00Z") as f64,
        now(),
    )
    .unwrap();
    assert!(!backup_plan(&db.conn, before).unwrap().due);
}

// describe("backupDatabase")
#[test]
fn copies_a_live_wal_db_into_one_file_and_keeps_only_the_latest_seven() {
    // it("쓰는 중인 WAL DB를 파일 하나로 옮기고, 최근 7개만 남긴다")
    let db = test_db();
    enable_wal(&db.conn).unwrap();
    write_str(&db.conn, &LLM_MODEL, "백업 확인용", now()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    // 예전 백업 8개와 상관없는 파일
    for day in 10..18 {
        std::fs::write(
            dir.path().join(format!("ripple-202610{day}-040000.db")),
            "old",
        )
        .unwrap();
    }
    std::fs::write(dir.path().join("memo.txt"), "keep").unwrap();

    let result =
        backup_database(db.conn.path().map(Path::new).unwrap(), dir.path(), now()).unwrap();

    assert_eq!(result.file, "ripple-20261022-040003.db");
    assert!(result.bytes > 0);
    // 예전 8개 + 새 1개 = 9개 중 오래된 2개를 지운다.
    assert_eq!(
        result.removed,
        ["ripple-20261011-040000.db", "ripple-20261010-040000.db"]
    );
    let names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(
        names.iter().filter(|n| n.starts_with("ripple-")).count(),
        BACKUP_KEEP
    );
    assert!(names.contains(&"memo.txt".to_string()));

    // 사본은 저널 모드 DELETE의 파일 하나이고, 내용이 그대로 있다.
    let copy = Connection::open_with_flags(
        dir.path().join(&result.file),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mode: String = copy
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "delete");
    let value: String = copy
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            ["llm.model"],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(value, serde_json::to_string("백업 확인용").unwrap());
    assert!(
        !names
            .iter()
            .any(|n| n.ends_with("-wal") || n.ends_with("-shm"))
    );
}

#[test]
fn removes_wal_shm_and_journal_leftovers_of_deleted_backups() {
    // TS: 지우는 이름마다 "", -wal, -shm, -journal을 force로 지운다.
    let db = test_db();
    let dir = tempfile::tempdir().unwrap();
    // 최근 6개 + 오래된 1개 + 새 1개 = 8개 → 가장 오래된 하나만 지운다.
    for day in 10..16 {
        std::fs::write(
            dir.path().join(format!("ripple-202610{day}-040000.db")),
            "old",
        )
        .unwrap();
    }
    for suffix in ["", "-wal", "-shm", "-journal"] {
        std::fs::write(
            dir.path()
                .join(format!("ripple-20261001-040000.db{suffix}")),
            "x",
        )
        .unwrap();
    }
    let result =
        backup_database(db.conn.path().map(Path::new).unwrap(), dir.path(), now()).unwrap();
    assert_eq!(result.removed, ["ripple-20261001-040000.db"]);
    let left: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.contains("20261001"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

#[test]
fn creates_the_backup_folder_when_missing() {
    // TS: mkdir(dir, { recursive: true })
    let db = test_db();
    let base = tempfile::tempdir().unwrap();
    let dir = base.path().join("a").join("backups");
    backup_database(db.conn.path().map(Path::new).unwrap(), &dir, now()).unwrap();
    assert!(dir.join("ripple-20261022-040003.db").is_file());
}

#[test]
fn fails_with_a_message_when_the_database_file_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope.db");
    let error = backup_database(&missing, dir.path(), now()).unwrap_err();
    assert!(matches!(error, BackupError::Sqlite(_)), "{error}");
}

#[tokio::test(flavor = "current_thread")]
async fn run_backup_does_the_same_work_on_a_blocking_thread() {
    let db = test_db();
    let dir = tempfile::tempdir().unwrap();
    let result = run_backup(
        db.conn.path().map(PathBuf::from).unwrap(),
        dir.path().to_path_buf(),
        now(),
    )
    .await
    .unwrap();
    assert_eq!(result.file, "ripple-20261022-040003.db");
}
