import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import { insertLink, seedListings } from "../../../tests/fixtures";
import { openDb, type Db } from "../db/client";
import { writeSetting } from "../settings";
import { registerStock } from "../stocks/repository";
import { MAX_HEADLINES, recentHeadlines } from "./headlines";

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

const NOW = new Date("2026-10-21T03:00:00Z");
const HOUR = 60 * 60 * 1000;
const ago = (hours: number) => new Date(NOW.getTime() - hours * HOUR);

function addStocks(db: Db) {
  const ids = ["005930", "000660"].map((code) => {
    const r = registerStock(db, { market: "KR", code, userInput: code });
    if (r.status === "not_listed") throw new Error("표본에 없음");
    return r.stock.id;
  });
  return { samsung: ids[0], hynix: ids[1] };
}

describe("recentHeadlines (FR-3 키워드 재생성 입력, 2026-10-21 결정)", () => {
  it("종목 이름·직접 연결이고 표시 기준(6점) 이상인 최근 7일 기사 제목을 최근순으로", () => {
    const db = testDb();
    const { samsung, hynix } = addStocks(db);
    const link = (
      title: string,
      hours: number,
      extra: Partial<Parameters<typeof insertLink>[1]> = {},
    ) =>
      insertLink(db, {
        stockId: samsung,
        title,
        publishedAt: ago(hours),
        relevance: 7,
        ...extra,
      });
    link("삼성전자 실적 발표", 30);
    link("삼성전자 노조 협상", 2);
    link("Samsung shares rise", 5, { kind: "direct" });
    link("기준 딱 6점", 6, { relevance: 6 });
    // 빠지는 것
    link("키워드로 걸린 기사", 1, { kind: "keyword" });
    link("관련도 낮은 기사", 1, { relevance: 5.9 });
    link("평가 전 기사", 1, { relevance: undefined });
    link("8일 지난 기사", 8 * 24);
    insertLink(db, {
      stockId: hynix,
      title: "다른 종목 기사",
      publishedAt: ago(1),
      relevance: 9,
    });

    expect(recentHeadlines(db, samsung, NOW)).toEqual([
      "삼성전자 노조 협상",
      "Samsung shares rise",
      "기준 딱 6점",
      "삼성전자 실적 발표",
    ]);

    writeSetting(db, "evaluation.minRelevance", 6.5);
    expect(recentHeadlines(db, samsung, NOW)).not.toContain("기준 딱 6점");
  });

  it("같은 제목(공백 차이 포함)은 하나만, 최대 20개", () => {
    const db = testDb();
    const { samsung } = addStocks(db);
    insertLink(db, {
      stockId: samsung,
      title: "같은  제목 ",
      publishedAt: ago(1),
      relevance: 8,
    });
    insertLink(db, {
      stockId: samsung,
      title: "같은 제목",
      publishedAt: ago(2),
      relevance: 8,
    });
    for (let i = 0; i < 30; i += 1) {
      insertLink(db, {
        stockId: samsung,
        title: `기사 ${i}`,
        publishedAt: ago(3 + i),
        relevance: 8,
      });
    }
    const titles = recentHeadlines(db, samsung, NOW);
    expect(titles).toHaveLength(MAX_HEADLINES);
    expect(titles.slice(0, 2)).toEqual(["같은 제목", "기사 0"]);
  });
});
