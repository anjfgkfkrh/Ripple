import { requireEnv } from "@/server/env";
import { openDb, type Db } from "./client";

const globalForDb = globalThis as typeof globalThis & { rippleWebDb?: Db };

/**
 * web(Next.js 서버)에서 쓰는 읽기 전용 DB. 처음 호출할 때 연다.
 * 개발 서버의 모듈 재로딩 때 연결이 쌓이지 않도록 globalThis에 둔다.
 */
export function getWebDb(): Db {
  globalForDb.rippleWebDb ??= openDb(requireEnv("DATABASE_PATH"), "readonly");
  return globalForDb.rippleWebDb;
}
