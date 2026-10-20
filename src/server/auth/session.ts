import { jwtVerify, SignJWT } from "jose";
import type { AuthConfig } from "./config";

export const SESSION_COOKIE = "ripple_session";
/** 세션 만료 30일, 접속해도 연장하지 않는다. (SPEC D-10) */
export const SESSION_MAX_AGE_SECONDS = 30 * 24 * 60 * 60;

/** DB 없이 서명한 쿠키로 세션을 표현한다(web은 DB에 쓰지 않음, SPEC D-36). */
export async function createSessionToken(
  config: AuthConfig,
  now: Date = new Date(),
): Promise<string> {
  const issuedAt = Math.floor(now.getTime() / 1000);
  return new SignJWT({ pwd: config.passwordFingerprint })
    .setProtectedHeader({ alg: "HS256" })
    .setIssuedAt(issuedAt)
    .setExpirationTime(issuedAt + SESSION_MAX_AGE_SECONDS)
    .sign(config.secretKey);
}

/** 서명·만료·비밀번호 지문이 모두 맞아야 유효하다. */
export async function isValidSessionToken(
  token: string | undefined,
  config: AuthConfig,
  now: Date = new Date(),
): Promise<boolean> {
  if (!token) return false;
  try {
    const { payload } = await jwtVerify(token, config.secretKey, {
      algorithms: ["HS256"],
      currentDate: now,
    });
    return payload.pwd === config.passwordFingerprint;
  } catch {
    return false;
  }
}
