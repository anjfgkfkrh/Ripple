import {
  cpSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { enableWal, openDb, type Db } from "./client";
import { applyMigrations } from "./migrate";

const MIGRATIONS = path.resolve("drizzle");
const opened: Db[] = [];
const dirs: string[] = [];

function tempDir(): string {
  const dir = mkdtempSync(path.join(tmpdir(), "ripple-migrate-"));
  dirs.push(dir);
  return dir;
}

function columnsOf(db: Db, table: string): string[] {
  return db.$client
    .prepare(`PRAGMA table_info(${table})`)
    .all()
    .map((column) => (column as { name: string }).name);
}

/** 실제 마이그레이션 폴더를 복사하고, 컬럼을 하나 추가하는 마이그레이션을 덧붙인다. */
function migrationsWithExtraColumn(): string {
  const folder = path.join(tempDir(), "drizzle");
  cpSync(MIGRATIONS, folder, { recursive: true });
  const journalPath = path.join(folder, "meta", "_journal.json");
  const journal = JSON.parse(readFileSync(journalPath, "utf8")) as {
    entries: { idx: number; when: number; tag: string }[];
  };
  const last = journal.entries.at(-1)!;
  const tag = `${String(last.idx + 1).padStart(4, "0")}_test_extra_column`;
  journal.entries.push({
    ...last,
    idx: last.idx + 1,
    when: last.when + 1,
    tag,
  });
  writeFileSync(journalPath, JSON.stringify(journal));
  writeFileSync(
    path.join(folder, `${tag}.sql`),
    "ALTER TABLE `settings` ADD `extra` text;",
  );
  return folder;
}

afterEach(() => {
  opened.splice(0).forEach((db) => db.$client.close());
  dirs
    .splice(0)
    .forEach((dir) => rmSync(dir, { recursive: true, force: true }));
});

describe("applyMigrations", () => {
  it("DB 파일과 상위 폴더가 없으면 만들고 settings 테이블을 만든다", () => {
    const dbPath = path.join(tempDir(), "nested", "ripple.db");
    applyMigrations(dbPath, MIGRATIONS);

    const db = openDb(dbPath, "readonly");
    opened.push(db);
    expect(columnsOf(db, "settings")).toEqual(["key", "value", "updated_at"]);
  });

  it("다시 실행해도 이미 적용한 것은 건너뛴다", () => {
    const dbPath = path.join(tempDir(), "ripple.db");
    applyMigrations(dbPath, MIGRATIONS);
    expect(() => applyMigrations(dbPath, MIGRATIONS)).not.toThrow();
  });

  it("web(읽기 전용)·worker(쓰기) 연결이 열려 있어도 새 마이그레이션을 적용한다", () => {
    const dbPath = path.join(tempDir(), "ripple.db");
    applyMigrations(dbPath, MIGRATIONS);
    const worker = openDb(dbPath, "readwrite");
    enableWal(worker);
    const web = openDb(dbPath, "readonly");
    opened.push(worker, web);
    // 두 연결 모두 실제로 한 번 읽어 둔 상태로 둔다.
    worker.$client.prepare("SELECT count(*) FROM settings").get();
    web.$client.prepare("SELECT count(*) FROM settings").get();

    const startedAt = Date.now();
    applyMigrations(dbPath, migrationsWithExtraColumn());

    // 잠금 대기(5초) 없이 바로 끝나야 한다.
    expect(Date.now() - startedAt).toBeLessThan(1000);
    expect(columnsOf(web, "settings")).toContain("extra");
  });
});
