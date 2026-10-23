import { eq } from "drizzle-orm";
import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import { insertLink, seedListings } from "../../../tests/fixtures";
import { openDb, type Db } from "../db/client";
import { stocks, topics } from "../db/schema";
import { writeSetting } from "../settings";
import { deleteStock, registerStock } from "../stocks/repository";
import {
  addUserTopic,
  attachTopicToStock,
  ensureDefaultMarket,
  setMarketEnabled,
  syncTopicLenses,
} from "../topics/store";
import {
  majorityImpact,
  parseTimelineParams,
  readTimeline,
  readTopicCards,
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

const all: TimelineFilter = { stockId: null, topicId: null, impact: null };
/** 서로 겹치는 두 글자가 없는 제목(비슷한 제목 묶기에 걸리지 않게) */
const distinct = (i: number) =>
  String.fromCharCode(0xac00 + i * 97) +
  String.fromCharCode(0xac00 + i * 97 + 41);
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

    const { cards, hasMore, minRelevance, minMagnitude } = read(db);
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
    expect(minMagnitude).toBe(6);

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
    const bySamsung = read(db, { ...all, stockId: ids.samsung }).cards;
    expect(bySamsung).toHaveLength(1);
    expect(bySamsung[0].stocks).toHaveLength(2);
    expect(titles(db, { ...all, stockId: ids.kakao })).toEqual([]);
    // 영향 필터: 그 영향인 종목이 하나라도 있으면 통과. 둘 다면 같은 종목이어야 한다.
    expect(titles(db, { ...all, impact: "positive" })).toHaveLength(1);
    expect(titles(db, { ...all, impact: "neutral" })).toEqual([]);
    expect(
      titles(db, { ...all, stockId: ids.samsung, impact: "positive" }),
    ).toEqual([]);
    expect(
      titles(db, { ...all, stockId: ids.samsung, impact: "negative" }),
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
        title: distinct(i),
        publishedAt: ago(i),
        relevance: 8,
      });
    }
    // 30번째 카드와 제목이 같은 기사가 31번째보다 뒤에 있다.
    insertLink(db, {
      stockId: ids.samsung,
      title: distinct(30),
      publisher: "나일보",
      publishedAt: ago(31.5),
      relevance: 8,
    });

    const first = read(db);
    expect(first.cards).toHaveLength(30);
    expect(first.hasMore).toBe(true);
    expect(first.cards[29]).toMatchObject({
      title: distinct(30),
      others: [{ publisher: "나일보" }],
    });

    const more = read(db, all, 60);
    expect(more.cards).toHaveLength(31);
    expect(more.hasMore).toBe(false);
  });
});

describe("readTimeline: 11단계", () => {
  it("관련도와 영향도가 둘 다 기준 이상이어야 보이고, 영향도가 없는 예전 평가는 관련도만 본다", () => {
    const db = testDb();
    const ids = addStocks(db);
    const add = (title: string, relevance: number, magnitude?: number) =>
      insertLink(db, {
        stockId: ids.samsung,
        title,
        publishedAt: ago(title.length),
        relevance,
        magnitude,
      });
    add("가", 8, 6);
    add("나나", 8, 5.9);
    add("다다다", 6, 9);
    add("라라라라", 7); // 11단계 전 평가

    expect(titles(db)).toEqual(["가", "다다다", "라라라라"]);
    expect(read(db).cards[0].stocks[0].magnitude).toBe(6);
    expect(read(db).cards[2].stocks[0].magnitude).toBeNull();
    writeSetting(db, "evaluation.minMagnitude", 7);
    expect(titles(db)).toEqual(["다다다", "라라라라"]);
  });

  it("비슷한 제목(같은 소식)은 한 카드로 묶고, 묶인 기사들의 항목을 모은다", () => {
    const db = testDb();
    const ids = addStocks(db);
    insertLink(db, {
      stockId: ids.samsung,
      title: "한은 기준금리 동결…연 2.5% 유지",
      publisher: "가경제",
      publishedAt: ago(1),
      relevance: 7,
      impact: "negative",
    });
    insertLink(db, {
      stockId: ids.hynix,
      title: "[속보] 한은, 기준금리 동결…연 2.5% 유지",
      publisher: "나일보",
      publishedAt: ago(2),
      relevance: 8,
      impact: "neutral",
    });
    // 같은 종목이 두 기사에 걸렸으면 관련도가 높은 것 하나만
    insertLink(db, {
      stockId: ids.samsung,
      title: "한국은행 기준금리 동결…연 2.5% 유지 결정",
      publisher: "다신문",
      publishedAt: ago(3),
      relevance: 9,
      impact: "positive",
    });
    insertLink(db, {
      stockId: ids.samsung,
      title: "카카오페이 새 서비스",
      publishedAt: ago(4),
      relevance: 8,
    });

    const { cards } = read(db);
    expect(cards.map((c) => c.title)).toEqual([
      "한은 기준금리 동결…연 2.5% 유지",
      "카카오페이 새 서비스",
    ]);
    expect(cards[0].others.map((o) => o.publisher)).toEqual([
      "나일보",
      "다신문",
    ]);
    expect(cards[0].stocks.map((s) => [s.name, s.relevance, s.impact])).toEqual(
      [
        ["삼성전자", 9, "positive"],
        ["SK하이닉스", 8, "neutral"],
      ],
    );
  });

  it("주제 필터는 그 주제의 증시별 항목을 모두 보고, 카드에 항목 종류를 담는다", () => {
    const db = testDb();
    const ids = addStocks(db);
    const market = ensureDefaultMarket(db, NOW)!;
    setMarketEnabled(db, "US", true, NOW);
    const topic = addUserTopic(db, "부동산 시장", NOW);
    if (!topic.ok) throw new Error("주제");
    const [kr, us] = topic.lensIds;
    const { articleId } = insertLink(db, {
      stockId: kr,
      title: "주택 공급 대책 발표",
      publishedAt: ago(1),
      kind: "keyword",
      path: "부동산 시장 → 주택 공급",
      relevance: 7,
      magnitude: 7,
    });
    insertLink(db, {
      stockId: us,
      articleId,
      publishedAt: ago(1),
      kind: "keyword",
      path: "부동산 시장 → 주택 공급",
      relevance: 6.5,
      magnitude: 6,
      impact: "positive",
    });
    insertLink(db, {
      stockId: market,
      title: "외국인 순매도 확대",
      publishedAt: ago(2),
      kind: "keyword",
      path: "한국 증시 → 외국인 수급",
      relevance: 8,
      magnitude: 8,
    });
    insertLink(db, {
      stockId: ids.samsung,
      title: "삼성전자 실적",
      publishedAt: ago(3),
      relevance: 8,
    });

    expect(titles(db, { ...all, topicId: topic.topicId })).toEqual([
      "주택 공급 대책 발표",
    ]);
    expect(titles(db, { ...all, stockId: market })).toEqual([
      "외국인 순매도 확대",
    ]);
    const [card] = read(db, { ...all, topicId: topic.topicId }).cards;
    expect(
      card.stocks.map((s) => [s.kind, s.market, s.topicId, s.topicSource]),
    ).toEqual([
      ["topic", "KR", topic.topicId, "user"],
      ["topic", "US", topic.topicId, "user"],
    ]);
    expect(read(db).cards[1].stocks[0]).toMatchObject({
      kind: "market",
      name: "한국 증시",
      topicId: null,
    });
  });
});

describe("majorityImpact", () => {
  const e = (
    impact: "positive" | "negative" | "neutral",
    relevance = 7,
    magnitude: number | null = 7,
  ) => ({
    impact,
    relevance,
    magnitude,
  });
  it("많은 쪽, 같으면 점수(관련도+영향도)가 가장 높은 평가 쪽", () => {
    expect(majorityImpact([])).toBeNull();
    expect(majorityImpact([e("positive"), e("negative"), e("negative")])).toBe(
      "negative",
    );
    expect(majorityImpact([e("positive", 9, 9), e("negative", 8, 8)])).toBe(
      "positive",
    );
    expect(majorityImpact([e("positive", 6, null), e("negative", 6, 1)])).toBe(
      "negative",
    );
  });
});

describe("readTopicCards (11단계)", () => {
  function autoTopic(db: Db, name: string, summary = `${name} 설명`) {
    const topic = db
      .insert(topics)
      .values({
        source: "auto",
        name,
        summary,
        terms: [name],
        createdAt: ago(80),
      })
      .returning()
      .get();
    return { topic, lenses: syncTopicLenses(db, topic, ago(80)) };
  }

  it("기준 이상 기사가 있는 자동 토픽만, 그런 기사가 많은 순으로. 12시간 넘게 새 기사가 없으면 접고 3일이 지나면 숨긴다", () => {
    const db = testDb();
    const ids = addStocks(db);
    ensureDefaultMarket(db, ago(80));
    const mideast = autoTopic(db, "중동 긴장");
    const rates = autoTopic(db, "기준금리");
    const faded = autoTopic(db, "미중 갈등");
    const hidden = autoTopic(db, "옛 토픽");
    const weak = autoTopic(db, "약한 토픽");
    const link = (
      lens: number,
      hours: number,
      relevance: number,
      impact: "positive" | "negative" | "neutral" = "negative",
    ) =>
      insertLink(db, {
        stockId: lens,
        title: distinct(Math.round(hours * 10) + lens * 1000),
        publishedAt: ago(hours),
        kind: "keyword",
        relevance,
        magnitude: 7,
        impact,
      });
    link(mideast.lenses[0], 1, 8);
    link(rates.lenses[0], 2, 8, "positive");
    link(rates.lenses[0], 3, 8, "positive");
    link(rates.lenses[0], 4, 8, "negative");
    link(rates.lenses[0], 5, 3); // 기준 미만은 세지 않음
    link(faded.lenses[0], 13, 8);
    link(hidden.lenses[0], 73, 8);
    link(weak.lenses[0], 1, 4);
    // 사용자 주제는 토픽 카드가 아니다(필터 칩).
    const user = addUserTopic(db, "부동산 시장", ago(80));
    if (!user.ok) throw new Error("주제");
    link(user.lensIds[0], 1, 9);

    // 현대건설 대신 삼성전자에 중동 긴장 가지를 붙인다.
    const samsung = db
      .select()
      .from(stocks)
      .where(eq(stocks.id, ids.samsung))
      .get()!;
    attachTopicToStock(
      db,
      samsung,
      mideast.topic,
      { reason: "중동 수주", relevance: 7, magnitude: 6 },
      ago(10),
    );
    insertLink(db, {
      stockId: ids.samsung,
      title: "사우디 수주 기대",
      publishedAt: ago(1),
      kind: "keyword",
      path: "삼성전자 → 중동 긴장 → 중동 긴장",
      relevance: 7,
      magnitude: 7,
      impact: "positive",
    });

    const cards = readTopicCards(db, NOW);
    expect(cards.map((c) => [c.name, c.faded, c.shownTotal])).toEqual([
      ["기준금리", false, 3],
      ["중동 긴장", false, 1],
      ["미중 갈등", true, 1],
    ]);
    expect(cards[0]).toMatchObject({
      summary: "기준금리 설명",
      shown24h: 3,
      impacts: [{ market: "KR", impact: "positive" }],
      related: [],
      lastArticleAt: ago(2),
    });
    expect(cards[1].related).toEqual([
      { stockId: ids.samsung, name: "삼성전자", impact: "positive" },
    ]);
  });
});

describe("parseTimelineParams", () => {
  it("모르는 종목·주제·영향은 무시하고, 개수는 30개 단위로 30~600", () => {
    expect(
      parseTimelineParams(
        { stock: "2", topic: "7", impact: "negative", limit: "45" },
        [1, 2],
        [7],
      ),
    ).toEqual({
      filter: { stockId: 2, topicId: 7, impact: "negative" },
      limit: 60,
    });
    expect(
      parseTimelineParams(
        { stock: "9", topic: "8", impact: "great", limit: "0" },
        [1],
        [7],
      ),
    ).toEqual({
      filter: { stockId: null, topicId: null, impact: null },
      limit: 30,
    });
    expect(parseTimelineParams({ limit: "99999" }, []).limit).toBe(
      TIMELINE_MAX,
    );
    expect(parseTimelineParams({ limit: ["60", "90"] }, []).limit).toBe(60);
    expect(parseTimelineParams({}, []).limit).toBe(TIMELINE_PAGE_SIZE);
  });
});
