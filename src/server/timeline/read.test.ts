import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import { insertLink, seedListings } from "../../../tests/fixtures";
import { openDb, type Db } from "../db/client";
import { writeSetting } from "../settings";
import { deleteStock, registerStock } from "../stocks/repository";
import {
  parseTimelineParams,
  readTimeline,
  TIMELINE_MAX,
  TIMELINE_PAGE_SIZE,
  type TimelineFilter,
} from "./read";

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
  const [samsung, hynix, kakao] = ["005930", "000660", "035720"].map((code) => {
    const r = registerStock(db, { market: "KR", code, userInput: code });
    if (r.status === "not_listed") throw new Error("표본에 없음");
    return r.stock.id;
  });
  return { samsung, hynix, kakao };
}

const all: TimelineFilter = { stockId: null, impact: null };
const read = (db: Db, filter = all, limit = TIMELINE_PAGE_SIZE) =>
  readTimeline(db, { filter, limit });
const titles = (db: Db, filter = all) =>
  read(db, filter).cards.map((card) => card.title);

describe("readTimeline (SPEC FR-7)", () => {
  it("관련도 기준 이상인 기사만 최신순으로, 삭제한 종목은 빼고", () => {
    const db = testDb();
    const ids = addStocks(db);
    insertLink(db, {
      stockId: ids.samsung,
      title: "삼성전자 실적",
      publishedAt: ago(1),
      relevance: 8,
    });
    insertLink(db, {
      stockId: ids.samsung,
      title: "기준 미만",
      publishedAt: ago(2),
      relevance: 5.9,
    });
    insertLink(db, {
      stockId: ids.hynix,
      title: "SK하이닉스 증설",
      publishedAt: ago(3),
      relevance: 6,
    });
    insertLink(db, {
      stockId: ids.samsung,
      title: "평가 전",
      publishedAt: ago(0.5),
    });
    insertLink(db, {
      stockId: ids.kakao,
      title: "삭제한 종목 기사",
      publishedAt: ago(0.2),
      relevance: 9,
    });
    deleteStock(db, ids.kakao);

    const { cards, hasMore, minRelevance } = read(db);
    expect(cards.map((c) => c.title)).toEqual([
      "삼성전자 실적",
      "SK하이닉스 증설",
    ]);
    expect(cards[0]).toMatchObject({
      publisher: "가경제",
      publishedAt: ago(1),
      others: [],
      stocks: [
        {
          stockId: ids.samsung,
          name: "삼성전자",
          relevance: 8,
          impact: "neutral",
          matchKind: "name",
        },
      ],
    });
    expect(hasMore).toBe(false);
    expect(minRelevance).toBe(6);

    writeSetting(db, "evaluation.minRelevance", 7);
    expect(titles(db)).toEqual(["삼성전자 실적"]);
  });

  it("여러 종목에 걸린 기사는 카드 하나에 기준 이상인 종목을 관련도 순으로(D-26)", () => {
    const db = testDb();
    const ids = addStocks(db);
    const { articleId } = insertLink(db, {
      stockId: ids.samsung,
      title: "삼성전자·SK하이닉스 HBM 경쟁",
      publishedAt: ago(1),
      kind: "keyword",
      path: "삼성전자 → HBM 메모리",
      relevance: 7,
      impact: "negative",
    });
    insertLink(db, {
      stockId: ids.hynix,
      articleId,
      publishedAt: ago(1),
      relevance: 9,
      impact: "positive",
    });
    insertLink(db, {
      stockId: ids.kakao,
      articleId,
      publishedAt: ago(1),
      relevance: 4,
    });

    const [card] = read(db).cards;
    expect(card.stocks.map((s) => [s.name, s.relevance, s.impact])).toEqual([
      ["SK하이닉스", 9, "positive"],
      ["삼성전자", 7, "negative"],
    ]);
    expect(card.stocks[1]).toMatchObject({
      matchKind: "keyword",
      path: "삼성전자 → HBM 메모리",
    });

    // 종목 필터: 그 종목이 카드에 있으면 통과하고, 카드에는 다른 종목도 그대로 보인다.
    const bySamsung = read(db, { stockId: ids.samsung, impact: null }).cards;
    expect(bySamsung).toHaveLength(1);
    expect(bySamsung[0].stocks).toHaveLength(2);
    expect(titles(db, { stockId: ids.kakao, impact: null })).toEqual([]);
    // 영향 필터: 그 영향인 종목이 하나라도 있으면 통과. 둘 다면 같은 종목이어야 한다.
    expect(titles(db, { stockId: null, impact: "positive" })).toHaveLength(1);
    expect(titles(db, { stockId: null, impact: "neutral" })).toEqual([]);
    expect(titles(db, { stockId: ids.samsung, impact: "positive" })).toEqual(
      [],
    );
    expect(
      titles(db, { stockId: ids.samsung, impact: "negative" }),
    ).toHaveLength(1);
  });

  it("제목이 같고 24시간 안이면 최근 기사 하나로 묶고, 나머지는 다른 언론사로", () => {
    const db = testDb();
    const ids = addStocks(db);
    const link = (publisher: string, hours: number, title = "삼성전자 실적") =>
      insertLink(db, {
        stockId: ids.samsung,
        title,
        publisher,
        publishedAt: ago(hours),
        relevance: 8,
      });
    link("가경제", 1);
    link("나일보", 5, " 삼성전자  실적 ");
    link("다신문", 3);
    // 가장 최근 것과 24시간 넘게 떨어지면 따로
    link("라뉴스", 30);

    const { cards } = read(db);
    expect(
      cards.map((c) => [c.publisher, c.others.map((o) => o.publisher)]),
    ).toEqual([
      ["가경제", ["다신문", "나일보"]],
      ["라뉴스", []],
    ]);
  });

  it("'더 보기': 개수만큼 보여 주고 더 있는지 알려 주며, 경계 너머의 같은 제목도 묶는다", () => {
    const db = testDb();
    const ids = addStocks(db);
    for (let i = 1; i <= 31; i += 1) {
      insertLink(db, {
        stockId: ids.samsung,
        title: `소식 ${i}`,
        publishedAt: ago(i),
        relevance: 8,
      });
    }
    // 30번째 카드와 제목이 같은 기사가 31번째보다 뒤에 있다.
    insertLink(db, {
      stockId: ids.samsung,
      title: "소식 30",
      publisher: "나일보",
      publishedAt: ago(31.5),
      relevance: 8,
    });

    const first = read(db);
    expect(first.cards).toHaveLength(30);
    expect(first.hasMore).toBe(true);
    expect(first.cards[29]).toMatchObject({
      title: "소식 30",
      others: [{ publisher: "나일보" }],
    });

    const more = read(db, all, 60);
    expect(more.cards).toHaveLength(31);
    expect(more.hasMore).toBe(false);
  });
});

describe("parseTimelineParams", () => {
  it("모르는 종목·영향은 무시하고, 개수는 30개 단위로 30~600", () => {
    expect(
      parseTimelineParams(
        { stock: "2", impact: "negative", limit: "45" },
        [1, 2],
      ),
    ).toEqual({ filter: { stockId: 2, impact: "negative" }, limit: 60 });
    expect(
      parseTimelineParams({ stock: "9", impact: "great", limit: "0" }, [1]),
    ).toEqual({ filter: { stockId: null, impact: null }, limit: 30 });
    expect(parseTimelineParams({ limit: "99999" }, []).limit).toBe(
      TIMELINE_MAX,
    );
    expect(parseTimelineParams({ limit: ["60", "90"] }, []).limit).toBe(60);
    expect(parseTimelineParams({}, []).limit).toBe(TIMELINE_PAGE_SIZE);
  });
});
