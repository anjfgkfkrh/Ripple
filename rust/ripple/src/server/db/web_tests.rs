use super::*;
use crate::test_support::test_db_file;

// Rust 전용: 파일이 아직 없으면 오류 글자를 돌려주고, 만들어진 뒤에는 열린다(성공한 연결만 보관)
#[test]
fn opens_lazily_and_retries_until_file_exists() {
    let file = test_db_file();
    let missing = file.path.with_file_name("later.db");
    let db = WebDb::new(Ok(missing.clone()));
    assert!(
        db.with(|c| c.query_row("SELECT 1", [], |r| r.get::<_, i64>(0)))
            .is_err()
    );
    std::fs::copy(&file.path, &missing).unwrap();
    assert_eq!(
        db.with(|c| c.query_row("SELECT 1", [], |r| r.get::<_, i64>(0))),
        Ok(1)
    );
}

// Rust 전용: 읽기 전용이라 쓰기는 실패한다
#[test]
fn connection_is_read_only() {
    let file = test_db_file();
    let db = WebDb::new(Ok(file.path.clone()));
    let result = db.with(|c| c.execute("CREATE TABLE zz (a)", []));
    assert!(result.is_err());
}

// Rust 전용: 경로 환경 변수가 없으면 그 글자를 그대로 오류로 돌려준다
#[test]
fn missing_path_reports_message() {
    let db = WebDb::new(Err(
        "환경변수 DATABASE_PATH이(가) 설정되지 않았습니다.".to_string()
    ));
    assert_eq!(
        db.with(|_| Ok(())).unwrap_err(),
        "환경변수 DATABASE_PATH이(가) 설정되지 않았습니다."
    );
}
