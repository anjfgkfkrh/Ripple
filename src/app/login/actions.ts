"use server";

import { cookies, headers } from "next/headers";
import { redirect } from "next/navigation";
import { getAuthConfig, isPasswordCorrect } from "@/server/auth/config";
import { getLoginRateLimiter } from "@/server/auth/rate-limit";
import { clientKey, isHttpsRequest, safeNextPath } from "@/server/auth/request";
import {
  createSessionToken,
  SESSION_COOKIE,
  SESSION_MAX_AGE_SECONDS,
} from "@/server/auth/session";
import { log } from "@/server/log";

export type LoginState = { error?: string };

export async function login(
  _previous: LoginState,
  formData: FormData,
): Promise<LoginState> {
  const requestHeaders = await headers();
  const key = clientKey(requestHeaders);
  const limiter = getLoginRateLimiter();

  // 막혀 있는 동안은 비밀번호를 확인하지 않는다.
  const blockedMs = limiter.blockedFor(key);
  if (blockedMs > 0) return { error: blockedMessage(blockedMs) };

  const config = getAuthConfig();
  const password = formData.get("password");
  if (
    typeof password !== "string" ||
    !isPasswordCorrect(password, config.password)
  ) {
    const nowBlockedMs = limiter.recordFailure(key);
    log(`로그인 실패 (${key})`);
    return {
      error:
        nowBlockedMs > 0
          ? blockedMessage(nowBlockedMs)
          : "비밀번호가 맞지 않습니다.",
    };
  }

  limiter.reset(key);
  (await cookies()).set(SESSION_COOKIE, await createSessionToken(config), {
    httpOnly: true,
    sameSite: "lax",
    secure: isHttpsRequest(requestHeaders),
    path: "/",
    maxAge: SESSION_MAX_AGE_SECONDS,
  });
  log(`로그인 성공 (${key})`);
  redirect(safeNextPath(formData.get("next")));
}

function blockedMessage(ms: number): string {
  return `로그인 시도가 너무 많습니다. ${Math.ceil(ms / 60_000)}분 뒤에 다시 시도하세요.`;
}
