import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import { seedListings } from "../../../tests/fixtures";
import type { FinnhubQuoteClient } from "../collect/finnhub";
import { openDb, type Db } from "../db/client";
import { stockPrices } from "../db/schema";
import { readSetting } from "../settings";
import { deleteStock, registerStock } from "../stocks/repository";
import { KisError, type KisClient } from "./kis";
import { createPriceRefresher } from "./run";
import { readPriceStrip } from "./store";

const opened: Db[] = [];
const cleanups: (() => void)[] = [];
afterEach(() => {
  opened.splice(0).forEach((db) => db.$client.close());
  cleanups.splice(0).forEach((cleanup) => cleanup());
});

const kst = (text: string) => new Date(`${text.replace(" ", "T")}:00+09:00`);

/** 한국 종목(삼성전자)과 미국 종목(NVDA)을 가진 시험 DB */
function seed() {
  const file = createTestDbFile();
  cleanups.push(file.cleanup);
  const db = openDb(file.path, "readwrite");
  opened.push(db);
  seedListings(db);
  const ids = [
    { market: "KR" as const, code: "005930", userInput: "삼성전자" },
    {
      market: "US" as const,
      code: "NVDA",
      userInput: "NVDA",
      nameKo: "엔비디아",
    },
  ].map((input) => {
    const result = registerStock(db, input);
    if (result.status === "not_listed") throw new Error("표본에 없음");
    return result.stock.id;
  });
  return { db, krId: ids[0], usId: ids[1] };
}

function fakeKis(price = 152300): KisClient & { codes: string[] } {
  const codes: string[] = [];
  return {
    codes,
    async domesticQuote(code) {
      codes.push(code);
      return { price, change: 2300, changePercent: 1.53 };
    },
  };
}

function fakeFinnhub(): FinnhubQuoteClient & { symbols: string[] } {
  const symbols: string[] = [];
  return {
    symbols,
    async quote(symbol) {
      symbols.push(symbol);
      return {
        price: 201.5,
        change: -3.25,
        changePercent: -1.59,
        quotedAt: new Date("2026-10-29T20:00:00Z"),
      };
    },
  };
}

describe("createPriceRefresher (13단계)", () => {
  it("장중인 시장의 종목만 받고, 한국은 KIS(시세 시각 없음)·미국은 Finnhub로 쌓는다", async () => {
    const { db, krId } = seed();
    const kis = fakeKis();
    const finnhub = fakeFinnhub();
    let now = kst("2026-10-30 10:00");
    const refresher = createPriceRefresher({
      db,
      kis,
      finnhub,
      now: () => now,
    });
    // 처음에는 장 밖인 미국 종목도 한 번 받는다(값이 하나도 없으므로).
    expect(await refresher.refresh()).toBe(true);
    expect(kis.codes).toEqual(["005930"]);
    expect(finnhub.symbols).toEqual(["NVDA"]);

    // 1분 뒤: 한국은 장중이라 또 받고, 미국은 장 밖이고 값이 있으니 건너뛴다.
    now = kst("2026-10-30 10:01");
    await refresher.refresh();
    expect(kis.codes).toHaveLength(2);
    expect(finnhub.symbols).toHaveLength(1);

    const rows = db.select().from(stockPrices).all();
    expect(rows).toHaveLength(3);
    expect(rows.find((row) => row.stockId === krId)).toMatchObject({
      price: 152300,
      change: 2300,
      changePercent: 1.53,
      source: "kis",
      quotedAt: null,
      fetchedAt: kst("2026-10-30 10:00"),
    });
    expect(readSetting(db, "prices.checkedAt")).toBe(
      kst("2026-10-30 10:01").getTime(),
    );
    expect(readSetting(db, "prices.error")).toBe("");
    expect(readSetting(db, "prices.note")).toBe("");
  });

  it("장 밖이고 모든 종목에 값이 있으면 아무것도 받지 않는다", async () => {
    const { db } = seed();
    const kis = fakeKis();
    const finnhub = fakeFinnhub();
    let now = kst("2026-10-31 10:00"); // 토요일
    const refresher = createPriceRefresher({
      db,
      kis,
      finnhub,
      now: () => now,
    });
    await refresher.refresh();
    now = kst("2026-10-31 10:01");
    expect(await refresher.refresh()).toBe(false);
    expect(kis.codes).toHaveLength(1);
    expect(finnhub.symbols).toHaveLength(1);
  });

  it("한 종목이 실패해도 나머지는 받고, 오류를 상태에 남겼다가 다음에 성공하면 지운다", async () => {
    const { db } = seed();
    let fail = true;
    const kis: KisClient = {
      async domesticQuote() {
        if (fail) throw new KisError("KIS 시세 오류(HTTP 500)");
        return { price: 1, change: 0, changePercent: 0 };
      },
    };
    const finnhub = fakeFinnhub();
    const refresher = createPriceRefresher({
      db,
      kis,
      finnhub,
      now: () => kst("2026-10-30 10:00"),
    });
    await refresher.refresh();
    expect(readSetting(db, "prices.error")).toBe(
      "삼성전자: KIS 시세 오류(HTTP 500)",
    );
    expect(db.select().from(stockPrices).all()).toHaveLength(1);
    fail = false;
    await refresher.refresh();
    expect(readSetting(db, "prices.error")).toBe("");
  });

  it("키가 없는 시장은 받지 않고 상태 화면용 안내를 남긴다. 지운 종목은 받지 않는다", async () => {
    const { db, usId } = seed();
    deleteStock(db, usId);
    const refresher = createPriceRefresher({
      db,
      kis: null,
      finnhub: fakeFinnhub(),
      now: () => kst("2026-10-30 10:00"),
    });
    expect(await refresher.refresh()).toBe(false);
    expect(readSetting(db, "prices.note")).toBe(
      "KIS 키(KIS_APP_KEY·KIS_APP_SECRET)가 없어 한국 종목 주가는 받지 않습니다.",
    );
    expect(db.select().from(stockPrices).all()).toEqual([]);
  });
});

describe("readPriceStrip (13단계)", () => {
  it("등록 순서대로 가장 최근 값과 장이 열렸는지를 돌려주고, 값이 없는 종목은 뺀다", () => {
    const { db, krId, usId } = seed();
    const base = {
      change: 0,
      changePercent: 0,
      source: "kis" as const,
      quotedAt: null,
    };
    db.insert(stockPrices)
      .values([
        {
          ...base,
          stockId: krId,
          price: 100,
          fetchedAt: kst("2026-10-30 10:00"),
        },
        {
          ...base,
          stockId: krId,
          price: 101,
          change: 1,
          changePercent: 1,
          fetchedAt: kst("2026-10-30 10:01"),
        },
      ])
      .run();
    expect(readPriceStrip(db, kst("2026-10-30 10:02"))).toEqual([
      {
        stockId: krId,
        name: "삼성전자",
        market: "KR",
        price: 101,
        change: 1,
        changePercent: 1,
        at: kst("2026-10-30 10:01"),
        open: true,
      },
    ]);
    db.insert(stockPrices)
      .values({
        ...base,
        stockId: usId,
        price: 201.5,
        source: "finnhub",
        quotedAt: new Date("2026-10-29T20:00:00Z"),
        fetchedAt: kst("2026-10-30 10:00"),
      })
      .run();
    const strip = readPriceStrip(db, kst("2026-10-30 21:00"));
    expect(strip.map((item) => [item.name, item.open])).toEqual([
      ["삼성전자", false],
      ["엔비디아", false],
    ]);
    // 미국은 Finnhub가 알려 준 시세 시각
    expect(strip[1].at).toEqual(new Date("2026-10-29T20:00:00Z"));
  });
});
