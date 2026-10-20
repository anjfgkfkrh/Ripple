/**
 * 로그인 실패 제한: 같은 키(IP)에서 15분 안에 5번 틀리면 15분 동안 막는다. (SPEC D-10)
 * 기록은 web 프로세스 메모리에만 둔다(web은 DB에 쓰지 않음, 재시작하면 초기화).
 */
export const MAX_FAILURES = 5;
export const BLOCK_MS = 15 * 60 * 1000;

export type LoginRateLimiter = {
  /** 막혀 있으면 남은 시간(ms), 아니면 0 */
  blockedFor(key: string, now?: number): number;
  /** 실패를 기록하고, 이번 실패로 막혔으면 남은 시간(ms), 아니면 0 */
  recordFailure(key: string, now?: number): number;
  /** 로그인에 성공하면 기록을 지운다. */
  reset(key: string): void;
  /** 기록 중인 키 수(메모리 사용 확인용) */
  size(): number;
};

type Entry = { failures: number[]; blockedUntil: number };

export function createLoginRateLimiter({
  maxFailures = MAX_FAILURES,
  windowMs = BLOCK_MS,
  blockMs = BLOCK_MS,
  maxKeys = 10_000,
} = {}): LoginRateLimiter {
  const entries = new Map<string, Entry>();

  // IP를 바꿔 가며 시도해도 메모리가 계속 늘지 않도록 끝난 기록을 정리한다.
  function prune(now: number) {
    if (entries.size < maxKeys) return;
    for (const [key, entry] of entries) {
      const recent = entry.failures.some((at) => now - at < windowMs);
      if (entry.blockedUntil <= now && !recent) entries.delete(key);
    }
  }

  return {
    blockedFor(key, now = Date.now()) {
      const entry = entries.get(key);
      return entry && entry.blockedUntil > now ? entry.blockedUntil - now : 0;
    },
    recordFailure(key, now = Date.now()) {
      prune(now);
      const entry = entries.get(key) ?? { failures: [], blockedUntil: 0 };
      entry.failures = entry.failures.filter((at) => now - at < windowMs);
      entry.failures.push(now);
      if (entry.failures.length >= maxFailures) {
        entry.blockedUntil = now + blockMs;
        entry.failures = [];
      }
      entries.set(key, entry);
      return Math.max(0, entry.blockedUntil - now);
    },
    reset(key) {
      entries.delete(key);
    },
    size() {
      return entries.size;
    },
  };
}

const globalForLimiter = globalThis as typeof globalThis & {
  rippleLoginLimiter?: LoginRateLimiter;
};

/** web 프로세스 하나에 하나. 개발 서버의 모듈 재로딩에도 유지되게 globalThis에 둔다. */
export function getLoginRateLimiter(): LoginRateLimiter {
  globalForLimiter.rippleLoginLimiter ??= createLoginRateLimiter();
  return globalForLimiter.rippleLoginLimiter;
}
