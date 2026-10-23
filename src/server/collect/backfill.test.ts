import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import { seedListings } from "../../../tests/fixtures";
import { openDb, type Db } from "../db/client";
import { articleMatches, articles } from "../db/schema";
import { replaceKeywords } from "../keywords/generate";
import { registerStock } from "../stocks/repository";
import { linkStoredHeadlines } from "./backfill";
import { saveFeedHeadlines } from "./feed-headlines";

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
const HOUR = 60 * 60 * 1000;
const ago = (hours: number) => new Date(NOW.getTime() - hours * HOUR);

function stockWithKeyword(db: Db, code: string, keyword: string): number {
  const registered = registerStock(db, { market: "KR", code, userInput: code });
  if (registered.status === "not_listed") throw new Error("표본");
  const id = registered.stock.id;
  const name = registered.stock.name;
  replaceKeywords(
    db,
    id,
    [
      {
        key: 1,
        parentKey: null,
        level: 1,
        kind: "structure",
        textKo: keyword,
        textEn: null,
        reason: "이유",
        path: `${name} → ${keyword}`,
      },
    ],
    NOW,
  );
  return id;
}

describe("linkStoredHeadlines (11단계)", () => {
  it("보관한 제목(원문 주소가 있는 것)을 주어진 항목에만, 기간 안의 것만 건다. 다시 해도 늘지 않는다", () => {
    const db = testDb();
    const samsung = stockWithKeyword(db, "005930", "HBM");
    const hynix = stockWithKeyword(db, "000660", "HBM");
    saveFeedHeadlines(
      db,
      "rss:test:economy",
      [
        {
          title: "HBM 증설 경쟁",
          publisher: "가경제",
          publishedAt: ago(2),
          url: "https://news.example/hbm",
          summary: "메모리 업계",
        },
        // 요약에만 키워드가 있어도 건다(제목+요약 앞 300자).
        {
          title: "메모리 업계 투자",
          publisher: "가경제",
          publishedAt: ago(3),
          url: "https://news.example/memory",
          summary: "HBM 라인을 늘린다",
        },
        // 주소가 없는 제목(11단계 전에 보관)은 걸 수 없다.
        { title: "HBM 수출 호조", publisher: "가경제", publishedAt: ago(1) },
        // 기간 밖
        {
          title: "HBM 지난 소식",
          publisher: "가경제",
          publishedAt: ago(30),
          url: "https://news.example/old",
        },
      ],
      NOW,
    );

    expect(linkStoredHeadlines(db, [samsung], ago(24), NOW)).toEqual({
      articles: 2,
      matches: 2,
    });
    expect(
      db
        .select({
          stockId: articleMatches.stockId,
          path: articleMatches.keywordPath,
        })
        .from(articleMatches)
        .all(),
    ).toEqual([
      { stockId: samsung, path: "삼성전자 → HBM" },
      { stockId: samsung, path: "삼성전자 → HBM" },
    ]);
    expect(db.select({ feed: articles.feed }).from(articles).all()).toEqual([
      { feed: "rss:test:economy" },
      { feed: "rss:test:economy" },
    ]);

    // 같은 기사를 다시 걸지 않고, 다른 항목에는 기사를 새로 만들지 않고 연결만 더한다.
    expect(linkStoredHeadlines(db, [samsung], ago(24), NOW)).toEqual({
      articles: 0,
      matches: 0,
    });
    expect(linkStoredHeadlines(db, [hynix], ago(24), NOW)).toEqual({
      articles: 0,
      matches: 2,
    });
    expect(linkStoredHeadlines(db, [], ago(24), NOW)).toEqual({
      articles: 0,
      matches: 0,
    });
  });
});
