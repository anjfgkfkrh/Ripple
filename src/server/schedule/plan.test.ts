import { eq } from "drizzle-orm";
import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import { seedListings } from "../../../tests/fixtures";
import { openDb, type Db } from "../db/client";
import {
  collectionRuns,
  stocks,
  type KeywordStatus,
  type RunStatus,
  type RunTrigger,
} from "../db/schema";
import { writeSetting } from "../settings";
import { deleteStock, registerStock } from "../stocks/repository";
import { collectionPlan, keywordPlan } from "./plan";

const opened: Db[] = [];
const cleanups: (() => void)[] = [];
function testDb(): Db {
  const file = createTestDbFile();
  cleanups.push(file.cleanup);
  const db = openDb(file.path, "readwrite");
  opened.push(db);
  seedListings(db);
  return db;
}
afterEach(() => {
  opened.splice(0).forEach((db) => db.$client.close());
  cleanups.splice(0).forEach((cleanup) => cleanup());
});

// 한국 시간 2026-10-21(수) 12:18
const NOW = new Date("2026-10-21T03:18:00Z");
const MIN = 60 * 1000;
const at = (minutes: number) => new Date(NOW.getTime() + minutes * MIN);

function addStock(db: Db, code = "005930"): number {
  const result = registerStock(db, {
    market: "KR",
    code,
    userInput: code,
  });
  if (result.status === "not_listed") throw new Error("표본에 없음");
  return result.stock.id;
}

function addRun(
  db: Db,
  startedAt: Date,
  status: RunStatus,
  trigger: RunTrigger = "scheduled",
) {
  db.insert(collectionRuns)
    .values({ trigger, status, startedAt, since: startedAt })
    .run();
}

describe("collectionPlan (D-01, D-24, D-31)", () => {
  it("등록한 종목이 없으면 쉰다(회차를 만들지 않음)", () => {
    const db = testDb();
    expect(collectionPlan(db, NOW)).toMatchObject({ idle: true, due: false });
    const id = addStock(db);
    deleteStock(db, id);
    expect(collectionPlan(db, NOW)).toMatchObject({ idle: true, due: false });
  });

  it("한 번도 돈 적이 없으면 바로 시작한다", () => {
    const db = testDb();
    addStock(db);
    expect(collectionPlan(db, NOW)).toEqual({
      intervalMinutes: 30,
      idle: false,
      running: false,
      nextAt: NOW,
      retrying: false,
      due: true,
      trigger: "scheduled",
    });
  });

  it("마지막으로 성공한 회차(일부 실패, 직접 실행 포함) 시작 + 30분", () => {
    const db = testDb();
    addStock(db);
    addRun(db, at(-50), "success");
    addRun(db, at(-20), "partial", "manual");
    expect(collectionPlan(db, NOW)).toMatchObject({
      nextAt: at(10),
      due: false,
    });
    expect(collectionPlan(db, at(10))).toMatchObject({
      due: true,
      trigger: "scheduled",
    });
  });

  it("예정보다 5분 넘게 늦으면(꺼져 있었거나 절전) 누락 보정으로 시작한다", () => {
    const db = testDb();
    addStock(db);
    addRun(db, at(-120), "success");
    expect(collectionPlan(db, NOW)).toMatchObject({
      nextAt: at(-90),
      due: true,
      trigger: "catch_up",
    });
    // 5분까지는 정기
    expect(collectionPlan(db, at(-85))).toMatchObject({
      due: true,
      trigger: "scheduled",
    });
  });

  it("회차 전체가 실패하면 5분 뒤 다시, 성공할 때까지 5분마다(D-24)", () => {
    const db = testDb();
    addStock(db);
    addRun(db, at(-60), "success");
    addRun(db, at(-2), "failed");
    expect(collectionPlan(db, NOW)).toMatchObject({
      nextAt: at(3),
      retrying: true,
      due: false,
    });
    addRun(db, at(3), "failed");
    expect(collectionPlan(db, at(4))).toMatchObject({
      nextAt: at(8),
      retrying: true,
    });
    // 처음부터 실패만 했어도 5분 뒤
    const fresh = testDb();
    addStock(fresh);
    addRun(fresh, at(-1), "failed");
    expect(collectionPlan(fresh, NOW)).toMatchObject({
      nextAt: at(4),
      retrying: true,
    });
  });

  it("실패가 성공보다 앞이거나 정기 시각이 더 늦으면 정기 시각을 따른다", () => {
    const db = testDb();
    addStock(db);
    addRun(db, at(-40), "failed");
    addRun(db, at(-10), "success");
    expect(collectionPlan(db, NOW)).toMatchObject({
      nextAt: at(20),
      retrying: false,
    });
    // 성공 직후 "지금 수집"이 실패해도 다음은 정기 시각
    addRun(db, at(-5), "failed", "manual");
    expect(collectionPlan(db, NOW)).toMatchObject({
      nextAt: at(20),
      retrying: false,
    });
  });

  it("도는 회차가 있으면 시작하지 않는다", () => {
    const db = testDb();
    addStock(db);
    addRun(db, at(-120), "success");
    addRun(db, at(-1), "running");
    expect(collectionPlan(db, NOW)).toMatchObject({
      running: true,
      due: false,
    });
  });

  it("간격 설정(15·30·60·180분)을 따르고, 그 밖의 값이면 30분", () => {
    const db = testDb();
    addStock(db);
    addRun(db, at(-10), "success");
    writeSetting(db, "collect.intervalMinutes", 60);
    expect(collectionPlan(db, NOW)).toMatchObject({
      intervalMinutes: 60,
      nextAt: at(50),
    });
    writeSetting(db, "collect.intervalMinutes", 45);
    expect(collectionPlan(db, NOW)).toMatchObject({
      intervalMinutes: 30,
      nextAt: at(20),
    });
  });
});

describe("keywordPlan (FR-3, 매일 06:00)", () => {
  // 이번 주기 시작: 한국 10-21 06:00 = 10-20T21:00Z
  const BOUNDARY = new Date("2026-10-20T21:00:00Z");

  function setKeywords(
    db: Db,
    id: number,
    status: KeywordStatus,
    generatedAt: Date | null,
  ) {
    db.update(stocks)
      .set({ keywordsStatus: status, keywordsGeneratedAt: generatedAt })
      .where(eq(stocks.id, id))
      .run();
  }

  it("이번 주기에 아직 안 했으면 06:00 전에 만든 키워드를 가진 종목을 다시 만든다", () => {
    const db = testDb();
    const old = addStock(db, "005930");
    const fresh = addStock(db, "000660");
    const waiting = addStock(db, "035720");
    const failed = addStock(db, "000720");
    const deleted = addStock(db, "179880");
    setKeywords(db, old, "ready", new Date("2026-10-20T10:00:00Z"));
    setKeywords(db, fresh, "ready", new Date("2026-10-21T01:00:00Z"));
    setKeywords(db, waiting, "pending", null);
    // 실패한 종목도 다음 주기에 다시(6단계 결정)
    setKeywords(db, failed, "failed", null);
    setKeywords(db, deleted, "ready", new Date("2026-10-19T00:00:00Z"));
    deleteStock(db, deleted);

    expect(keywordPlan(db, NOW)).toEqual({
      every: "daily",
      idle: false,
      nextAt: NOW,
      due: true,
      stockIds: [old, failed],
    });
  });

  it("이번 주기에 이미 했으면 다음 날 06:00", () => {
    const db = testDb();
    addStock(db);
    writeSetting(db, "keywords.autoRegeneratedAt", BOUNDARY.getTime() + MIN);
    expect(keywordPlan(db, NOW)).toMatchObject({
      due: false,
      stockIds: [],
      nextAt: new Date("2026-10-21T21:00:00Z"),
    });
    // 다음 날 06:00이 되면 다시
    expect(keywordPlan(db, new Date("2026-10-21T21:00:00Z")).due).toBe(true);
  });

  it("일주일 1회면 월요일 06:00", () => {
    const db = testDb();
    addStock(db);
    writeSetting(db, "keywords.regenerateEvery", "weekly");
    // 이번 주 월요일(10-19) 06:00 = 10-18T21:00Z 뒤에 이미 했다.
    writeSetting(
      db,
      "keywords.autoRegeneratedAt",
      new Date("2026-10-18T22:00:00Z").getTime(),
    );
    expect(keywordPlan(db, NOW)).toMatchObject({
      every: "weekly",
      due: false,
      nextAt: new Date("2026-10-25T21:00:00Z"),
    });
  });

  it("종목이 없으면 idle(주기 표시는 그대로)", () => {
    const db = testDb();
    expect(keywordPlan(db, NOW)).toMatchObject({
      idle: true,
      due: true,
      stockIds: [],
    });
  });
});
