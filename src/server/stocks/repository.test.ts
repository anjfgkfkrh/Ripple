import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import { seedListings } from "../../../tests/fixtures";
import { openDb, type Db } from "../db/client";
import { deleteStock, listActiveStocks, registerStock } from "./repository";

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

describe("종목 등록 (SPEC FR-2)", () => {
  it("공식 목록 값으로 저장하고 사용자 입력을 남긴다", () => {
    const db = testDb();
    const result = registerStock(db, {
      market: "US",
      code: "nvda",
      userInput: " 엔비디아 ",
      nameKo: "엔비디아",
    });
    expect(result.status).toBe("created");
    expect(listActiveStocks(db)).toEqual([
      expect.objectContaining({
        market: "US",
        code: "NVDA",
        name: "NVIDIA Corporation - Common Stock",
        nameKo: "엔비디아",
        exchange: "NASDAQ",
        userInput: "엔비디아",
        deletedAt: null,
      }),
    ]);
  });

  it("한국 종목의 한글 이름은 정식 이름을 쓴다", () => {
    const db = testDb();
    registerStock(db, {
      market: "KR",
      code: "005930",
      userInput: "삼전",
      nameKo: "삼성",
    });
    expect(listActiveStocks(db)[0]?.nameKo).toBe("삼성전자");
  });

  it("목록에 없으면 등록하지 않는다(D-12)", () => {
    const db = testDb();
    expect(
      registerStock(db, { market: "KR", code: "123456", userInput: "x" })
        .status,
    ).toBe("not_listed");
    expect(listActiveStocks(db)).toEqual([]);
  });

  it("이미 등록했으면 exists", () => {
    const db = testDb();
    registerStock(db, { market: "KR", code: "005930", userInput: "삼전" });
    expect(
      registerStock(db, { market: "KR", code: "005930", userInput: "삼성" })
        .status,
    ).toBe("exists");
    expect(listActiveStocks(db)).toHaveLength(1);
  });
});

describe("종목 삭제 (SPEC D-11: 숨기고 보존)", () => {
  it("지우지 않고 숨기며, 다시 등록하면 같은 종목을 되살린다", () => {
    const db = testDb();
    const created = registerStock(db, {
      market: "KR",
      code: "005930",
      userInput: "삼전",
    });
    if (created.status !== "created") throw new Error("등록 실패");
    const now = new Date("2026-10-20T00:00:00Z");

    expect(deleteStock(db, created.stock.id, now)).toBe(true);
    expect(listActiveStocks(db)).toEqual([]);
    expect(deleteStock(db, created.stock.id)).toBe(false);

    const restored = registerStock(db, {
      market: "KR",
      code: "005930",
      userInput: "삼성전자",
    });
    expect(restored.status).toBe("restored");
    if (restored.status !== "restored") return;
    expect(restored.stock.id).toBe(created.stock.id);
    expect(restored.stock.userInput).toBe("삼성전자");
    expect(listActiveStocks(db)).toHaveLength(1);
  });

  it("없는 종목은 false", () => {
    expect(deleteStock(testDb(), 999)).toBe(false);
  });
});
