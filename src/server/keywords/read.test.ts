import { eq } from "drizzle-orm";
import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import { seedListings } from "../../../tests/fixtures";
import { openDb, type Db } from "../db/client";
import { stocks, topics } from "../db/schema";
import { listActiveStocks, registerStock } from "../stocks/repository";
import {
  addUserTopic,
  attachTopicToStock,
  ensureDefaultMarket,
  syncTopicLenses,
} from "../topics/store";
import { replaceKeywords } from "./generate";
import { stocksWithOtherDepth } from "./read";
import type { KeywordNode } from "./tree";

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

const NOW = new Date("2026-10-22T06:00:00Z");

/** 깊이 2 트리(1단계 하나, 2단계 하나) */
function depthTwo(name: string): KeywordNode[] {
  return [
    {
      key: 1,
      parentKey: null,
      level: 1,
      kind: "structure",
      textKo: "가",
      textEn: null,
      reason: "",
      path: `${name} → 가`,
    },
    {
      key: 2,
      parentKey: 1,
      level: 2,
      kind: "structure",
      textKo: "나",
      textEn: null,
      reason: "",
      path: `${name} → 가 → 나`,
    },
  ];
}

describe("stocksWithOtherDepth (11단계)", () => {
  it("시장·사용자 주제도 다시 만들고, 자동 토픽 항목과 토픽 가지는 깊이에 넣지 않는다", () => {
    const db = testDb();
    const registered = registerStock(db, {
      market: "KR",
      code: "005930",
      userInput: "삼전",
    });
    if (registered.status === "not_listed") throw new Error("표본");
    const samsung = registered.stock;
    const market = ensureDefaultMarket(db, NOW)!;
    const user = addUserTopic(db, "부동산 시장", NOW);
    if (!user.ok) throw new Error("주제");
    const auto = db
      .insert(topics)
      .values({
        source: "auto",
        name: "중동 긴장",
        terms: ["사우디"],
        createdAt: NOW,
      })
      .returning()
      .get();
    syncTopicLenses(db, auto, NOW);

    replaceKeywords(db, samsung.id, depthTwo("삼성전자"), NOW);
    replaceKeywords(db, market, depthTwo("한국 증시"), NOW);
    replaceKeywords(
      db,
      user.lensIds[0],
      depthTwo("부동산 시장").slice(0, 1),
      NOW,
    );
    // 토픽 가지(1·2단계)는 트리 깊이에 넣지 않는다(아래에서 깊이 1 트리로 확인).
    attachTopicToStock(
      db,
      samsung,
      auto,
      { reason: "중동 수주", relevance: 7, magnitude: 6 },
      NOW,
    );

    // 기본 깊이 2: 깊이 1인 사용자 주제만 다시 만든다(자동 토픽 항목은 1단계뿐이어도 뺀다).
    expect(stocksWithOtherDepth(db)).toEqual([user.lensIds[0]]);

    // 깊이 1 트리에 2단계까지인 토픽 가지가 붙어 있어도 깊이 1로 본다.
    replaceKeywords(db, samsung.id, depthTwo("삼성전자").slice(0, 1), NOW);
    expect(stocksWithOtherDepth(db)).toEqual([samsung.id, user.lensIds[0]]);
  });

  it("listActiveStocks는 종목만 돌려준다(시장·주제 항목은 뺀다)", () => {
    const db = testDb();
    ensureDefaultMarket(db, NOW);
    addUserTopic(db, "부동산 시장", NOW);
    expect(listActiveStocks(db)).toEqual([]);
    const registered = registerStock(db, {
      market: "KR",
      code: "005930",
      userInput: "삼전",
    });
    if (registered.status === "not_listed") throw new Error("표본");
    expect(listActiveStocks(db).map((s) => s.id)).toEqual([
      registered.stock.id,
    ]);
    expect(
      db.select().from(stocks).where(eq(stocks.kind, "stock")).all(),
    ).toHaveLength(1);
  });
});
