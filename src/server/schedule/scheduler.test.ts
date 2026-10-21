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
import { readSetting } from "../settings";
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
  options: { jobsEnabled?: boolean; collector?: CollectionRunner } = {},
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
});
