import { mkdtempSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import Database from "better-sqlite3";
import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../tests/db";
import {
  BACKUP_KEEP,
  backupDatabase,
  backupFileName,
  backupPlan,
} from "./backup";
import { enableWal, openDb, type Db } from "./db/client";
import { writeSetting } from "./settings";

const opened: Db[] = [];
const cleanups: (() => void)[] = [];
function testDb(): Db {
  const file = createTestDbFile();
  cleanups.push(file.cleanup);
  const db = openDb(file.path, "readwrite");
  opened.push(db);
  return db;
}
function tempDir(): string {
  const dir = mkdtempSync(path.join(tmpdir(), "ripple-backup-"));
  cleanups.push(() => rmSync(dir, { recursive: true, force: true }));
  return dir;
}
afterEach(() => {
  opened.splice(0).forEach((db) => db.$client.close());
  cleanups.splice(0).forEach((cleanup) => cleanup());
});

// 한국 시간 2026-10-22(목) 04:00:03
const NOW = new Date("2026-10-21T19:00:03Z");

describe("backupFileName", () => {
  it("한국 시간으로 이름을 붙여 이름 순서가 시간 순서다", () => {
    expect(backupFileName(NOW)).toBe("ripple-20261022-040003.db");
  });
});

describe("backupPlan (D-27: 매일 04:00)", () => {
  it("오늘 04:00 뒤에 아직 하지 않았으면 지금, 했으면 내일 04:00", () => {
    const db = testDb();
    expect(backupPlan(db, NOW)).toEqual({ due: true, nextAt: NOW });
    writeSetting(db, "backup.lastAt", NOW.getTime());
    expect(backupPlan(db, NOW)).toEqual({
      due: false,
      nextAt: new Date("2026-10-22T19:00:00Z"),
    });
    // 04:00 전(한국 03:59)이면 어제 04:00 뒤에 했는지만 본다.
    const before = new Date("2026-10-21T18:59:00Z");
    writeSetting(
      db,
      "backup.lastAt",
      new Date("2026-10-20T19:30:00Z").getTime(),
    );
    expect(backupPlan(db, before).due).toBe(false);
  });
});

describe("backupDatabase", () => {
  it("쓰는 중인 WAL DB를 파일 하나로 옮기고, 최근 7개만 남긴다", async () => {
    const db = testDb();
    enableWal(db);
    writeSetting(db, "llm.model", "백업 확인용");
    const dir = tempDir();
    // 예전 백업 8개와 상관없는 파일
    for (let day = 10; day < 18; day += 1) {
      writeFileSync(path.join(dir, `ripple-202610${day}-040000.db`), "old");
    }
    writeFileSync(path.join(dir, "memo.txt"), "keep");

    const result = await backupDatabase(db, dir, NOW);

    expect(result.file).toBe("ripple-20261022-040003.db");
    expect(result.bytes).toBeGreaterThan(0);
    // 예전 8개 + 새 1개 = 9개 중 오래된 2개를 지운다.
    expect(result.removed).toEqual([
      "ripple-20261011-040000.db",
      "ripple-20261010-040000.db",
    ]);
    const left = readdirSync(dir).filter((name) => name.startsWith("ripple-"));
    expect(left).toHaveLength(BACKUP_KEEP);
    expect(readdirSync(dir)).toContain("memo.txt");

    // 사본은 저널 모드 DELETE의 파일 하나이고, 내용이 그대로 있다.
    const copy = new Database(path.join(dir, result.file), { readonly: true });
    try {
      expect(copy.pragma("journal_mode", { simple: true })).toBe("delete");
      expect(
        copy
          .prepare("select value from settings where key = ?")
          .get("llm.model"),
      ).toEqual({ value: JSON.stringify("백업 확인용") });
    } finally {
      copy.close();
    }
  });
});
