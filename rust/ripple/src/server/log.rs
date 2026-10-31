use crate::server::time::{EpochMs, format_seoul_date_time, now_ms};

/// `[YYYY-MM-DD HH:mm:ss] 문구`(한국 시간). 시각은 인자로 받아 테스트할 수 있게 한다.
pub fn format_log_line(now: EpochMs, message: &str) -> String {
    format!("[{}] {message}", format_seoul_date_time(now))
}

pub fn log(message: &str) {
    println!("{}", format_log_line(now_ms(), message));
}

/// TS는 오류 객체를 그대로 찍지만 Rust는 `{error}`(Display)로 찍는다(허용된 차이, 설계 3.8).
pub fn log_error(message: &str, error: &dyn std::fmt::Display) {
    eprintln!("{} {error}", format_log_line(now_ms(), message));
}

#[cfg(test)]
#[path = "log_tests.rs"]
mod tests;
