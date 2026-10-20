import { describe, expect, it } from "vitest";
import { BLOCK_MS, createLoginRateLimiter, MAX_FAILURES } from "./rate-limit";

const MINUTE = 60 * 1000;

describe("로그인 실패 제한 (SPEC D-10: 5회 실패 시 15분 차단)", () => {
  it("4번 틀릴 때까지는 막지 않고, 5번째에 15분 막는다", () => {
    const limiter = createLoginRateLimiter();
    for (let i = 1; i < MAX_FAILURES; i++) {
      expect(limiter.recordFailure("ip:a", i)).toBe(0);
      expect(limiter.blockedFor("ip:a", i)).toBe(0);
    }
    expect(limiter.recordFailure("ip:a", MAX_FAILURES)).toBe(BLOCK_MS);
    expect(limiter.blockedFor("ip:a", MAX_FAILURES + 1)).toBe(BLOCK_MS - 1);
  });

  it("15분이 지나면 풀리고, 다시 5번 틀려야 막힌다", () => {
    const limiter = createLoginRateLimiter();
    for (let i = 0; i < MAX_FAILURES; i++) limiter.recordFailure("ip:a", 0);
    expect(limiter.blockedFor("ip:a", BLOCK_MS - 1)).toBe(1);
    expect(limiter.blockedFor("ip:a", BLOCK_MS)).toBe(0);
    expect(limiter.recordFailure("ip:a", BLOCK_MS)).toBe(0);
  });

  it("15분보다 오래된 실패는 세지 않는다", () => {
    const limiter = createLoginRateLimiter();
    for (let i = 0; i < MAX_FAILURES - 1; i++) limiter.recordFailure("ip:a", 0);
    expect(limiter.recordFailure("ip:a", 16 * MINUTE)).toBe(0);
  });

  it("IP마다 따로 세고, 성공하면 기록을 지운다", () => {
    const limiter = createLoginRateLimiter();
    for (let i = 0; i < MAX_FAILURES; i++) limiter.recordFailure("ip:a", 0);
    expect(limiter.blockedFor("ip:a", 1)).toBeGreaterThan(0);
    expect(limiter.blockedFor("ip:b", 1)).toBe(0);

    for (let i = 0; i < MAX_FAILURES - 1; i++) limiter.recordFailure("ip:c", 0);
    limiter.reset("ip:c");
    expect(limiter.recordFailure("ip:c", 1)).toBe(0);
  });

  it("기록이 너무 많아지면 끝난 기록부터 정리하고, 아직 막힌 기록은 남긴다", () => {
    const limiter = createLoginRateLimiter({ maxKeys: 3 });
    limiter.recordFailure("ip:old1", 0);
    limiter.recordFailure("ip:old2", 0);
    // 10분에 막힘 → 25분까지 차단
    for (let i = 0; i < MAX_FAILURES; i++) {
      limiter.recordFailure("ip:blocked", 10 * MINUTE);
    }
    expect(limiter.size()).toBe(3);

    // 16분: old1·old2는 15분이 지나 끝난 기록이라 정리된다.
    limiter.recordFailure("ip:new", 16 * MINUTE);
    expect(limiter.size()).toBe(2);
    expect(limiter.blockedFor("ip:blocked", 16 * MINUTE)).toBe(9 * MINUTE);
  });
});
