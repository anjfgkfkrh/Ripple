import { eq } from "drizzle-orm";
import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import {
  fakeCollector,
  fakeKeywordQueue,
  seedListings,
} from "../../../tests/fixtures";
import type { CollectionRunner } from "../collect/runner";
import { openDb, type Db } from "../db/client";
import { collectionRuns, stocks } from "../db/schema";
import { readSetting, writeSetting } from "../settings";
import { registerStock } from "../stocks/repository";
import { startScheduler, type Scheduler } from "./scheduler";

const opened: Db[] = [];
const cleanups: (() => void)[] = [];
const schedulers: Scheduler[] = [];
function testDb(): Db {
  const file = createTestDbFile();
  cleanups.push(file.cleanup);
  const db = openDb(file.path, "readwrite");
  opened.push(db);
  seedListings(db);
  return db;
}
afterEach(() => {
  schedulers.splice(0).forEach((s) => s.stop());
  opened.splice(0).forEach((db) => db.$client.close());
  cleanups.splice(0).forEach((cleanup) => cleanup());
});

// 한국 시간 2026-10-21(수) 12:18
const NOW = new Date("2026-10-21T03:18:00Z");
const MIN = 60 * 1000;

function setup(
  options: {
    jobsEnabled?: boolean;
    collector?: CollectionRunner;
    backup?: (now: Date) => Promise<{ file: string; bytes: number }>;
    refreshTopics?: () => Promise<boolean>;
  } = {},
) {
  const db = testDb();
  const registered = registerStock(db, {
    market: "KR",
    code: "005930",
    userInput: "삼전",
  });
  if (registered.status === "not_listed") throw new Error("표본에 없음");
  const stockId = registered.stock.id;
  // 이번 주기(06:00) 전에 만든 키워드
  db.update(stocks)
    .set({
      keywordsStatus: "ready",
      keywordsGeneratedAt: new Date("2026-10-20T10:00:00Z"),
    })
    .where(eq(stocks.id, stockId))
    .run();
  const collector = fakeCollector();
  const keywordQueue = fakeKeywordQueue();
  const listingChecks: Date[] = [];
  let now = NOW;
  const start = () => {
    const scheduler = startScheduler({
      db,
      collector: options.collector ?? collector,
      keywordQueue,
      syncListings: async () => {
        listingChecks.push(now);
      },
      jobsEnabled: options.jobsEnabled,
      backup: options.backup,
      refreshTopics: options.refreshTopics,
      now: () => now,
      // 테스트에서는 tick()을 직접 부른다.
      tickMs: 24 * 60 * MIN,
    });
    schedulers.push(scheduler);
    return scheduler;
  };
  return {
    db,
    stockId,
    collector,
    keywordQueue,
    listingChecks,
    start,
    now: () => now,
    advance(minutes: number) {
      now = new Date(now.getTime() + minutes * MIN);
    },
  };
}

describe("startScheduler (SPEC FR-6, D-31)", () => {
  it("시작하자마자 놓친 수집을 누락 보정으로 하고, 놓친 키워드 재생성과 목록 확인도 한다", () => {
    const t = setup();
    t.db
      .insert(collectionRuns)
      .values({
        trigger: "scheduled",
        status: "success",
        startedAt: new Date(NOW.getTime() - 120 * MIN),
        since: NOW,
      })
      .run();

    t.start();

    expect(t.collector.started).toEqual(["catch_up"]);
    expect(t.keywordQueue.enqueued).toEqual([t.stockId]);
    expect(readSetting(t.db, "keywords.autoRegeneratedAt")).toBe(NOW.getTime());
    expect(t.listingChecks).toEqual([NOW]);
  });

  it("간격이 지나기 전에는 시작하지 않고, 지나면 정기 수집을 시작한다", () => {
    const t = setup();
    t.db
      .insert(collectionRuns)
      .values({
        trigger: "manual",
        status: "success",
        startedAt: new Date(NOW.getTime() - 10 * MIN),
        since: NOW,
      })
      .run();
    const scheduler = t.start();
    expect(t.collector.started).toEqual([]);

    t.advance(19);
    scheduler.tick();
    expect(t.collector.started).toEqual([]);
    t.advance(1);
    scheduler.tick();
    expect(t.collector.started).toEqual(["scheduled"]);
  });

  it("키워드 자동 재생성은 주기마다 한 번만 한다", () => {
    const t = setup();
    const scheduler = t.start();
    t.advance(1);
    scheduler.tick();
    expect(t.keywordQueue.enqueued).toEqual([t.stockId]);
  });

  it("공식 목록 확인은 30분마다", () => {
    const t = setup();
    const scheduler = t.start();
    t.advance(29);
    scheduler.tick();
    expect(t.listingChecks).toHaveLength(1);
    t.advance(1);
    scheduler.tick();
    expect(t.listingChecks).toHaveLength(2);
  });

  it("정기 작업을 끄면(E2E) 수집·키워드는 하지 않고 목록 확인만 한다", () => {
    const t = setup({ jobsEnabled: false });
    t.start();
    expect(t.collector.started).toEqual([]);
    expect(t.keywordQueue.enqueued).toEqual([]);
    expect(t.listingChecks).toHaveLength(1);
  });

  it("수집 시작이 실패해도 키워드 재생성과 목록 확인은 한다", () => {
    const t = setup({
      collector: {
        start: () => {
          throw new Error("시작 실패");
        },
        runningId: () => null,
        idle: () => Promise.resolve(),
      },
    });
    t.start();
    expect(t.keywordQueue.enqueued).toEqual([t.stockId]);
    expect(t.listingChecks).toHaveLength(1);
  });

  it("백업 폴더가 있으면 04:00 주기마다 한 번 백업하고 기록한다(정기 작업을 꺼도)", async () => {
    const calls: Date[] = [];
    const t = setup({
      jobsEnabled: false,
      backup: async (at) => {
        calls.push(at);
        return { file: "ripple-20261021-121800.db", bytes: 1024 };
      },
    });
    const scheduler = t.start();
    await new Promise((resolve) => setImmediate(resolve));
    expect(calls).toEqual([NOW]);
    expect(readSetting(t.db, "backup.lastAt")).toBe(NOW.getTime());
    expect(readSetting(t.db, "backup.lastFile")).toBe(
      "ripple-20261021-121800.db",
    );
    t.advance(60);
    scheduler.tick();
    await new Promise((resolve) => setImmediate(resolve));
    expect(calls).toHaveLength(1);
  });

  it("백업이 실패하면 1분마다 되풀이하지 않고 1시간 뒤에 다시 한다", async () => {
    let calls = 0;
    const t = setup({
      backup: async () => {
        calls += 1;
        throw new Error("폴더에 쓸 수 없음");
      },
    });
    const scheduler = t.start();
    await new Promise((resolve) => setImmediate(resolve));
    t.advance(30);
    scheduler.tick();
    await new Promise((resolve) => setImmediate(resolve));
    expect(calls).toBe(1);
    t.advance(31);
    scheduler.tick();
    await new Promise((resolve) => setImmediate(resolve));
    expect(calls).toBe(2);
    expect(readSetting(t.db, "backup.lastAt")).toBe(0);
  });

  it("토픽 갱신(11단계): 처음이면 바로, 그 뒤에는 간격마다. 실패하면 30분 뒤에 다시 하고, 도는 동안은 겹쳐 시작하지 않는다", async () => {
    let ok = false;
    let finish: (() => void) | null = null;
    const calls: Date[] = [];
    // 실제 갱신처럼 시작·끝(성공 또는 실패)을 설정에 남긴다.
    const t = setup({
      refreshTopics: () => {
        const at = t.now();
        calls.push(at);
        writeSetting(t.db, "topics.startedAt", at.getTime());
        return new Promise<boolean>((resolve) => {
          finish = () => {
            writeSetting(
              t.db,
              ok ? "topics.refreshedAt" : "topics.failedAt",
              at.getTime(),
            );
            resolve(ok);
          };
        });
      },
    });
    const flush = () => new Promise((resolve) => setImmediate(resolve));
    const scheduler = t.start();
    expect(calls).toEqual([NOW]);
    // 도는 동안에는 다시 시작하지 않는다.
    t.advance(1);
    scheduler.tick();
    expect(calls).toHaveLength(1);
    finish!();
    await flush();

    // 실패: 30분이 지나야 다시 한다.
    t.advance(28);
    scheduler.tick();
    expect(calls).toHaveLength(1);
    t.advance(1);
    scheduler.tick();
    expect(calls).toHaveLength(2);
    ok = true;
    finish!();
    await flush();

    // 성공 뒤에는 간격(기본 3시간)마다
    t.advance(179);
    scheduler.tick();
    expect(calls).toHaveLength(2);
    t.advance(1);
    scheduler.tick();
    expect(calls).toHaveLength(3);
  });

  it("토픽 갱신을 끄면(간격 0·시각 없음) 하지 않는다", () => {
    const calls: Date[] = [];
    const t = setup({
      refreshTopics: async () => {
        calls.push(new Date());
        return true;
      },
    });
    writeSetting(t.db, "topics.intervalHours", 0);
    t.start();
    expect(calls).toEqual([]);
  });
});
