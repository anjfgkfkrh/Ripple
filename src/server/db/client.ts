import path from "node:path";
import Database from "better-sqlite3";
import { drizzle } from "drizzle-orm/better-sqlite3";
import * as schema from "./schema";

/** DB 쓰기는 worker만 한다. web은 읽기 전용으로 연다. (SPEC D-36) */
export type DbAccess = "readonly" | "readwrite";

export type Db = ReturnType<typeof openDb>;

/** 다른 연결이 잠금을 잡고 있을 때 기다리는 시간(SQLite busy_timeout) */
const BUSY_TIMEOUT_MS = 5000;

export function openDb(databasePath: string, access: DbAccess) {
  const sqlite = new Database(path.resolve(databasePath), {
    readonly: access === "readonly",
    // DB 파일은 마이그레이션(migrate 서비스)이 만든다. 없으면 새로 만들지 않고 실패한다.
    fileMustExist: true,
    timeout: BUSY_TIMEOUT_MS,
  });
  return drizzle(sqlite, { schema });
}

/**
 * WAL 모드를 켠다. WAL은 DB 파일에 저장되는 설정이라 쓰는 쪽(worker)이 시작할 때 한 번 하면 된다.
 * WAL에서는 읽기와 쓰기가 서로 막지 않는다.
 */
export function enableWal(db: Db): void {
  const mode = db.$client.pragma("journal_mode = WAL", { simple: true });
  if (mode !== "wal") {
    throw new Error(`WAL 모드를 켜지 못했습니다(현재: ${String(mode)}).`);
  }
}

export function readJournalMode(db: Db): string {
  return String(db.$client.pragma("journal_mode", { simple: true }));
}
