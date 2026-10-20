import { describe, expect, it } from "vitest";
import { readAuthConfig } from "./config";
import {
  createSessionToken,
  isValidSessionToken,
  SESSION_MAX_AGE_SECONDS,
} from "./session";

const env = {
  APP_PASSWORD: "correct-horse-battery",
  SESSION_SECRET: "test-session-secret-0123456789abcdef",
};
const config = readAuthConfig(env);
const DAY = 24 * 60 * 60 * 1000;

describe("세션 토큰", () => {
  it("만든 토큰은 유효하다", async () => {
    const token = await createSessionToken(config);
    expect(await isValidSessionToken(token, config)).toBe(true);
  });

  it("만료는 30일: 29일 뒤에는 유효, 31일 뒤에는 무효", async () => {
    expect(SESSION_MAX_AGE_SECONDS).toBe(30 * 24 * 60 * 60);
    const issuedAt = new Date("2026-10-19T00:00:00Z");
    const token = await createSessionToken(config, issuedAt);
    const later = (days: number) => new Date(issuedAt.getTime() + days * DAY);
    expect(await isValidSessionToken(token, config, later(29))).toBe(true);
    expect(await isValidSessionToken(token, config, later(31))).toBe(false);
  });

  it("서명 키가 다르면 무효", async () => {
    const token = await createSessionToken(config);
    const other = readAuthConfig({
      ...env,
      SESSION_SECRET: "another-session-secret-0123456789ab",
    });
    expect(await isValidSessionToken(token, other)).toBe(false);
  });

  it("비밀번호를 바꾸면 기존 토큰은 무효 (SPEC FR-1)", async () => {
    const token = await createSessionToken(config);
    const changed = readAuthConfig({
      ...env,
      APP_PASSWORD: "new-correct-horse-battery",
    });
    expect(await isValidSessionToken(token, changed)).toBe(false);
  });

  it("빈 값·변조·서명 없는 토큰은 무효", async () => {
    const token = await createSessionToken(config);
    const [header, payload, signature] = token.split(".");
    const tampered = `${header}.${payload}.${signature.slice(0, -2)}AA`;
    const unsigned = `${Buffer.from('{"alg":"none"}').toString("base64url")}.${payload}.`;

    expect(await isValidSessionToken(undefined, config)).toBe(false);
    expect(await isValidSessionToken("", config)).toBe(false);
    expect(await isValidSessionToken("not-a-token", config)).toBe(false);
    expect(await isValidSessionToken(tampered, config)).toBe(false);
    expect(await isValidSessionToken(unsigned, config)).toBe(false);
  });
});
