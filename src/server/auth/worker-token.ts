import { createHash, timingSafeEqual } from "node:crypto";
import { MIN_SECRET_LENGTH } from "./config";

type Env = Record<string, string | undefined>;

/**
 * web → worker 내부 API 공유 토큰(SPEC 8장). web·worker 둘 다 같은 값을 쓴다.
 * 없거나 짧으면 시작하지 않는다. 값은 오류 메시지에 넣지 않는다.
 */
export function readWorkerApiToken(env: Env = process.env): string {
  const token = env.WORKER_API_TOKEN ?? "";
  if (token.length < MIN_SECRET_LENGTH) {
    throw new Error(
      `WORKER_API_TOKEN은 ${MIN_SECRET_LENGTH}자 이상이어야 합니다.`,
    );
  }
  return token;
}

/** "Bearer <토큰>" 헤더가 맞는지 같은 시간에 비교한다. */
export function isAuthorizedBearer(
  header: string | undefined,
  token: string,
): boolean {
  const presented = header?.startsWith("Bearer ") ? header.slice(7) : "";
  const a = createHash("sha256").update(presented).digest();
  const b = createHash("sha256").update(token).digest();
  return presented.length > 0 && timingSafeEqual(a, b);
}
