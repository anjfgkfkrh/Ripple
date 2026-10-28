import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import { insertLink, seedListings } from "../../../tests/fixtures";
import { openDb, type Db } from "../db/client";
import { articleReads } from "../db/schema";
import { registerStock } from "../stocks/repository";
import { readArticleIds, setArticlesRead } from "./reads";

const opened: Db[] = [];
const cleanups: (() => void)[] = [];
afterEach(() => {
  opened.splice(0).forEach((db) => db.$client.close());
  cleanups.splice(0).forEach((cleanup) => cleanup());
});

/** 기사 n개가 걸린 종목 하나를 가진 시험 DB */
function seed(count: number) {
  const file = createTestDbFile();
  cleanups.push(file.cleanup);
  const db = openDb(file.path, "readwrite");
  opened.push(db);
  seedListings(db);
  const result = registerStock(db, {
    market: "KR",
    code: "005930",
    userInput: "삼성전자",
  });
  if (result.status === "not_listed") throw new Error("표본에 없음");
  const articleIds = Array.from(
    { length: count },
    (_, i) =>
      insertLink(db, {
        stockId: result.stock.id,
        publishedAt: new Date(Date.UTC(2026, 9, 21, 0, i)),
      }).articleId,
  );
  return { db, articleIds };
}

describe("setArticlesRead (12단계)", () => {
  it("읽음으로 저장하고 읽은 시각을 남기며, 다시 읽음으로 해도 처음 시각을 지킨다", () => {
    const { db, articleIds } = seed(2);
    const first = new Date("2026-10-28T01:00:00Z");
    expect(setArticlesRead(db, [articleIds[0]], true, first)).toBe(1);
    expect(setArticlesRead(db, [articleIds[0]], true, new Date())).toBe(0);
    expect(db.select().from(articleReads).all()).toEqual([
      { articleId: articleIds[0], readAt: first },
    ]);
    expect(readArticleIds(db, articleIds)).toEqual(new Set([articleIds[0]]));
  });

  it("안 읽음으로 되돌리면 지우고, 읽지 않은 기사는 그대로 둔다", () => {
    const { db, articleIds } = seed(3);
    setArticlesRead(db, articleIds, true);
    expect(setArticlesRead(db, [articleIds[0], articleIds[1]], false)).toBe(2);
    expect(readArticleIds(db, articleIds)).toEqual(new Set([articleIds[2]]));
    expect(setArticlesRead(db, [articleIds[0]], false)).toBe(0);
  });

  it("같은 id가 겹쳐도 한 번만 저장하고, 없는 기사 id는 건너뛴다", () => {
    const { db, articleIds } = seed(1);
    expect(
      setArticlesRead(db, [articleIds[0], articleIds[0], 9999], true),
    ).toBe(1);
    expect(setArticlesRead(db, [9999], true)).toBe(0);
    expect(setArticlesRead(db, [], true)).toBe(0);
    expect(db.select().from(articleReads).all()).toHaveLength(1);
  });

  it("기사를 지우면 읽음 기록도 함께 지워진다", () => {
    const { db, articleIds } = seed(1);
    setArticlesRead(db, articleIds, true);
    db.$client.pragma("foreign_keys = ON");
    db.$client.prepare("DELETE FROM article_matches").run();
    db.$client.prepare("DELETE FROM evaluations").run();
    db.$client.prepare("DELETE FROM articles").run();
    expect(db.select().from(articleReads).all()).toEqual([]);
  });
});
