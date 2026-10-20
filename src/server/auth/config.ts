import { createHash, createHmac, timingSafeEqual } from "node:crypto";

/** 비밀 값 최소 길이. 이보다 짧으면 web이 시작하지 않는다. (SPEC 8장) */
export const MIN_PASSWORD_LENGTH = 7;
export const MIN_SECRET_LENGTH = 32;

export type AuthConfig = {
  password: string;
  /** 세션 서명 키(HS256) */
  secretKey: Uint8Array;
  /**
   * 세션에 넣는 비밀번호 지문. 비밀번호를 바꾸면 값이 달라져 기존 로그인이 모두 풀린다.
   * 서명 키로 만든 HMAC이라 쿠키가 새어도 비밀번호를 거꾸로 알아낼 수 없다.
   */
  passwordFingerprint: string;
};

type Env = Record<string, string | undefined>;

/** 값은 오류 메시지에 넣지 않는다. */
export function readAuthConfig(env: Env = process.env): AuthConfig {
  const password = env.APP_PASSWORD ?? "";
  const secret = env.SESSION_SECRET ?? "";
  if (password.length < MIN_PASSWORD_LENGTH) {
    throw new Error(
      `APP_PASSWORD는 ${MIN_PASSWORD_LENGTH}자 이상이어야 합니다.`,
    );
  }
  if (secret.length < MIN_SECRET_LENGTH) {
    throw new Error(
      `SESSION_SECRET은 ${MIN_SECRET_LENGTH}자 이상이어야 합니다.`,
    );
  }
  return {
    password,
    secretKey: new TextEncoder().encode(secret),
    passwordFingerprint: createHmac("sha256", secret)
      .update(`app-password:${password}`)
      .digest("base64url"),
  };
}

let cached: AuthConfig | undefined;

export function getAuthConfig(): AuthConfig {
  cached ??= readAuthConfig();
  return cached;
}

/** 길이와 내용에 상관없이 같은 시간에 비교한다(시간 차로 비밀번호를 추측하지 못하게). */
export function isPasswordCorrect(input: string, expected: string): boolean {
  const a = createHash("sha256").update(input).digest();
  const b = createHash("sha256").update(expected).digest();
  return timingSafeEqual(a, b);
}
