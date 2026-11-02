//! 로그인 실패 제한(`auth/rate-limit.ts`): 같은 키(IP)에서 15분 안에 5번 틀리면 15분 동안 막는다. (SPEC D-10)
//! 기록은 web 프로세스 메모리에만 둔다(web은 DB에 쓰지 않음, 재시작하면 초기화).
use std::collections::HashMap;
use std::sync::Mutex;

use crate::server::time::EpochMs;

pub const MAX_FAILURES: usize = 5;
pub const BLOCK_MS: i64 = 15 * 60 * 1000;
const DEFAULT_MAX_KEYS: usize = 10_000;

#[derive(Default)]
struct Entry {
    failures: Vec<EpochMs>,
    blocked_until: EpochMs,
}

pub struct LoginRateLimiter {
    max_failures: usize,
    window_ms: i64,
    block_ms: i64,
    max_keys: usize,
    entries: Mutex<HashMap<String, Entry>>,
}

impl Default for LoginRateLimiter {
    fn default() -> Self {
        Self::new(MAX_FAILURES, BLOCK_MS, BLOCK_MS, DEFAULT_MAX_KEYS)
    }
}

impl LoginRateLimiter {
    pub fn new(max_failures: usize, window_ms: i64, block_ms: i64, max_keys: usize) -> Self {
        Self {
            max_failures,
            window_ms,
            block_ms,
            max_keys,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// 막혀 있으면 남은 시간(ms), 아니면 0
    pub fn blocked_for(&self, key: &str, now: EpochMs) -> i64 {
        let entries = self.entries.lock().expect("제한 기록 잠금이 깨졌습니다");
        match entries.get(key) {
            Some(entry) if entry.blocked_until > now => entry.blocked_until - now,
            _ => 0,
        }
    }

    /// 실패를 기록하고, 이번 실패로 막혔으면 남은 시간(ms), 아니면 0
    pub fn record_failure(&self, key: &str, now: EpochMs) -> i64 {
        let mut entries = self.entries.lock().expect("제한 기록 잠금이 깨졌습니다");
        // IP를 바꿔 가며 시도해도 메모리가 계속 늘지 않도록 끝난 기록을 정리한다.
        if entries.len() >= self.max_keys {
            let window = self.window_ms;
            entries.retain(|_, entry| {
                let recent = entry.failures.iter().any(|at| now - at < window);
                !(entry.blocked_until <= now && !recent)
            });
        }
        let entry = entries.entry(key.to_string()).or_default();
        entry.failures.retain(|at| now - at < self.window_ms);
        entry.failures.push(now);
        if entry.failures.len() >= self.max_failures {
            entry.blocked_until = now + self.block_ms;
            entry.failures.clear();
        }
        (entry.blocked_until - now).max(0)
    }

    /// 로그인에 성공하면 기록을 지운다.
    pub fn reset(&self, key: &str) {
        self.entries
            .lock()
            .expect("제한 기록 잠금이 깨졌습니다")
            .remove(key);
    }

    /// 기록 중인 키 수(메모리 사용 확인용)
    pub fn size(&self) -> usize {
        self.entries
            .lock()
            .expect("제한 기록 잠금이 깨졌습니다")
            .len()
    }
}

#[cfg(test)]
#[path = "rate_limit_tests.rs"]
mod tests;
