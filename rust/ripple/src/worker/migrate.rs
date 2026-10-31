//! 마이그레이션 적용 진입점(`ripple migrate`). Docker에서는 compose의 migrate 서비스가 실행한다.
use std::path::Path;

use crate::server::db::migrate::apply_migrations;
use crate::server::env::{Env, require_env};
use crate::server::log::{log, log_error};

/// 종료 코드를 돌려준다(성공 0, 실패 1).
pub fn run(env: &Env) -> i32 {
    let result = (|| -> Result<String, Box<dyn std::error::Error>> {
        let database_path = require_env("DATABASE_PATH", env)?;
        apply_migrations(Path::new(&database_path))?;
        Ok(database_path)
    })();
    match result {
        Ok(database_path) => {
            log(&format!("마이그레이션 적용 완료: {database_path}"));
            0
        }
        Err(error) => {
            log_error("마이그레이션 실패", &error);
            1
        }
    }
}
