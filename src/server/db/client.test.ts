import path from "node:path";
import { eq } from "drizzle-orm";
import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import { enableWal, openDb, readJournalMode, type Db } from "./client";
import { settings } from "./schema";

const opened: Db[] = [];
const cleanups: (() => void)[] = [];

function open(dbPath: string, access: "readonly" | "readwrite") {
  const db = openDb(dbPath, access);
  opened.push(db);
  return db;
}

function testDb() {
  const file = createTestDbFile();
  cleanups.push(file.cleanup);
  return file;
}

function countSettings(db: Db): number {
  return db.select().from(settings).all().length;
}

afterEach(() => {
  opened.splice(0).forEach((db) => db.$client.close());
  cleanups.splice(0).forEach((cleanup) => cleanup());
});

describe("openDb", () => {
  it("마이그레이션으로 만든 settings 테이블을 읽고 쓴다", () => {
    const db = open(testDb().path, "readwrite");
    db.insert(settings)
      .values({ key: "test", value: JSON.stringify(1) })
      .run();
    const row = db
      .select()
      .from(settings)
      .where(eq(settings.key, "test"))
      .get();
    expect(JSON.parse(row!.value)).toBe(1);
    expect(row!.updatedAt).toBeInstanceOf(Date);
  });

  it("수정하면 updatedAt이 갱신된다", async () => {
    const db = open(testDb().path, "readwrite");
    db.insert(settings).values({ key: "k", value: "1" }).run();
    const before = db.select().from(settings).get()!.updatedAt;
    await new Promise((resolve) => setTimeout(resolve, 5));
    db.update(settings).set({ value: "2" }).where(eq(settings.key, "k")).run();
    const after = db.select().from(settings).get()!.updatedAt;
    expect(after.getTime()).toBeGreaterThan(before.getTime());
  });

  it("DB 파일이 없으면 새로 만들지 않고 바로 실패한다", () => {
    const { dir } = testDb();
    expect(() => openDb(path.join(dir, "missing.db"), "readwrite")).toThrow();
  });
});

describe("web(읽기 전용) / worker(쓰기) 연결 (SPEC D-36)", () => {
  it("worker가 WAL을 켜면 DB 파일에 남아 web 연결에서도 보인다", () => {
    const { path: dbPath } = testDb();
    const worker = open(dbPath, "readwrite");
    enableWal(worker);
    const web = open(dbPath, "readonly");
    expect(readJournalMode(web)).toBe("wal");
  });

  it("읽기 전용 연결은 읽기는 되고 쓰기는 거부된다", () => {
    const { path: dbPath } = testDb();
    const worker = open(dbPath, "readwrite");
    enableWal(worker);
    worker.insert(settings).values({ key: "a", value: '"x"' }).run();

    const web = open(dbPath, "readonly");
    expect(countSettings(web)).toBe(1);
    expect(() =>
      web.insert(settings).values({ key: "b", value: '"y"' }).run(),
    ).toThrow(/readonly/i);
    expect(countSettings(worker)).toBe(1);
  });

  it("WAL에서는 쓰기 트랜잭션 중에도 읽기가 막히지 않고, 커밋 전 내용은 보이지 않는다", () => {
    const { path: dbPath } = testDb();
    const worker = open(dbPath, "readwrite");
    enableWal(worker);
    const web = open(dbPath, "readonly");

    worker.transaction((tx) => {
      tx.insert(settings).values({ key: "pending", value: "1" }).run();
      expect(countSettings(web)).toBe(0);
    });

    expect(countSettings(web)).toBe(1);
  });
});
