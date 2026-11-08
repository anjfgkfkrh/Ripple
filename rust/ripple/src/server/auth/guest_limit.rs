//! 손님 요청 제한: 같은 키(IP)가 1분에 60번까지만 손님 GET을 보낼 수 있다. (결정 ①, 세부 ①)
//! 1분 고정 창으로 센다. 기록은 web 프로세스 메모리에만 두고(재시작하면 초기화), 키는 최대 10,000개다.
use std::collections::HashMap;
use std::sync::Mutex;

use crate::server::time::EpochMs;

pub const MAX_REQUESTS: u32 = 60;
pub const WINDOW_MS: i64 = 60 * 1000;
const DEFAULT_MAX_KEYS: usize = 10_000;

struct Entry {
    window_start: EpochMs,
    count: u32,
}

pub struct GuestRateLimiter {
    max_requests: u32,
    window_ms: i64,
    max_keys: usize,
    entries: Mutex<HashMap<String, Entry>>,
}

impl Default for GuestRateLimiter {
    fn default() -> Self {
        Self::new(MAX_REQUESTS, WINDOW_MS, DEFAULT_MAX_KEYS)
    }
}

impl GuestRateLimiter {
    pub fn new(max_requests: u32, window_ms: i64, max_keys: usize) -> Self {
        Self {
            max_requests,
            window_ms,
            max_keys,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// 요청 하나를 센다. 통과하면 `Ok`, 넘었으면 다시 시도할 수 있을 때까지 남은 초(1 이상)를 `Err`로 돌려준다.
    pub fn check(&self, key: &str, now: EpochMs) -> Result<(), u64> {
        let mut entries = self.entries.lock().expect("제한 기록 잠금이 깨졌습니다");
        if !entries.contains_key(key) && entries.len() >= self.max_keys {
            // IP를 바꿔 가며 보내도 메모리가 늘지 않도록 지난 창부터 지운다.
            let window = self.window_ms;
            entries.retain(|_, entry| now - entry.window_start < window);
            if entries.len() >= self.max_keys {
                return Err(self.retry_after(entries.values().map(|e| e.window_start).min(), now));
            }
        }
        let entry = entries.entry(key.to_string()).or_insert(Entry {
            window_start: now,
            count: 0,
        });
        if now - entry.window_start >= self.window_ms {
            entry.window_start = now;
            entry.count = 0;
        }
        if entry.count >= self.max_requests {
            return Err(self.retry_after(Some(entry.window_start), now));
        }
        entry.count += 1;
        Ok(())
    }

    fn retry_after(&self, window_start: Option<EpochMs>, now: EpochMs) -> u64 {
        let remaining = window_start.map_or(self.window_ms, |start| start + self.window_ms - now);
        (remaining.max(1) as u64).div_ceil(1000).max(1)
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
#[path = "guest_limit_tests.rs"]
mod tests;
