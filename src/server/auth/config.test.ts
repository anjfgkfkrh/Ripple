import { describe, expect, it } from "vitest";
import {
  isPasswordCorrect,
  MIN_PASSWORD_LENGTH,
  MIN_SECRET_LENGTH,
  readAuthConfig,
} from "./config";

const PASSWORD = "correct-horse-battery";
const SECRET = "s".repeat(MIN_SECRET_LENGTH);

describe("readAuthConfig", () => {
  it("비밀번호·서명 키가 충분히 길면 설정을 만든다", () => {
    const config = readAuthConfig({
      APP_PASSWORD: PASSWORD,
      SESSION_SECRET: SECRET,
    });
    expect(config.password).toBe(PASSWORD);
    expect(config.secretKey.length).toBe(MIN_SECRET_LENGTH);
  });

  it.each([
    [{ SESSION_SECRET: SECRET }, "APP_PASSWORD"],
    [
      {
        APP_PASSWORD: "x".repeat(MIN_PASSWORD_LENGTH - 1),
        SESSION_SECRET: SECRET,
      },
      "APP_PASSWORD",
    ],
    [{ APP_PASSWORD: PASSWORD }, "SESSION_SECRET"],
    [
      {
        APP_PASSWORD: PASSWORD,
        SESSION_SECRET: "s".repeat(MIN_SECRET_LENGTH - 1),
      },
      "SESSION_SECRET",
    ],
  ])("없거나 짧으면 실패한다 (%o)", (env, name) => {
    expect(() => readAuthConfig(env)).toThrow(name);
  });

  it("오류 메시지에 값을 넣지 않는다", () => {
    const shortSecret = "short-secret-value";
    expect(() =>
      readAuthConfig({ APP_PASSWORD: PASSWORD, SESSION_SECRET: shortSecret }),
    ).toThrow(expect.not.stringContaining(shortSecret) as unknown as string);
  });

  it("비밀번호를 바꾸면 지문이 바뀌고, 지문에 비밀번호가 그대로 들어가지 않는다", () => {
    const a = readAuthConfig({
      APP_PASSWORD: PASSWORD,
      SESSION_SECRET: SECRET,
    });
    const b = readAuthConfig({
      APP_PASSWORD: `${PASSWORD}!`,
      SESSION_SECRET: SECRET,
    });
    expect(a.passwordFingerprint).not.toBe(b.passwordFingerprint);
    expect(a.passwordFingerprint).not.toContain(PASSWORD);
  });
});

describe("최소 길이 (SPEC 12장: 비밀번호 7자, 서명 키 32자)", () => {
  it("비밀번호는 7자부터 받고 6자는 거부한다", () => {
    expect(MIN_PASSWORD_LENGTH).toBe(7);
    expect(() =>
      readAuthConfig({ APP_PASSWORD: "1234567", SESSION_SECRET: SECRET }),
    ).not.toThrow();
    expect(() =>
      readAuthConfig({ APP_PASSWORD: "123456", SESSION_SECRET: SECRET }),
    ).toThrow("APP_PASSWORD는 7자 이상");
  });

  it("서명 키는 32자부터 받는다", () => {
    expect(MIN_SECRET_LENGTH).toBe(32);
  });
});

describe("isPasswordCorrect", () => {
  it("같으면 참, 다르면(길이가 달라도) 거짓", () => {
    expect(isPasswordCorrect(PASSWORD, PASSWORD)).toBe(true);
    expect(isPasswordCorrect(`${PASSWORD} `, PASSWORD)).toBe(false);
    expect(isPasswordCorrect("", PASSWORD)).toBe(false);
    expect(isPasswordCorrect("x", PASSWORD)).toBe(false);
  });
});
