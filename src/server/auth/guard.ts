import "server-only";
import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import { connection } from "next/server";
import { getAuthConfig } from "./config";
import { isValidSessionToken, SESSION_COOKIE } from "./session";

export async function hasValidSession(): Promise<boolean> {
  // 만료 확인에 현재 시각이 필요하므로 요청 시점에만 실행한다(프리렌더 제외).
  await connection();
  const token = (await cookies()).get(SESSION_COOKIE)?.value;
  return isValidSessionToken(token, getAuthConfig());
}

/**
 * 페이지·서버 액션에서 데이터에 닿기 전에 부른다.
 * proxy.ts의 1차 확인과 별개로 한 번 더 확인한다(Next.js 권장, SPEC FR-1).
 */
export async function requireSession(): Promise<void> {
  if (!(await hasValidSession())) redirect("/login");
}
