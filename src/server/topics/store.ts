import { and, asc, eq, inArray, isNotNull, isNull } from "drizzle-orm";
import type { Db } from "@/server/db/client";
import {
  keywords,
  MARKETS,
  stocks,
  topics,
  type Market,
} from "@/server/db/schema";
import { PATH_SEPARATOR } from "@/server/keywords/tree";
import { NEW_STOCK_BACKFILL_MS } from "@/server/stocks/repository";

/**
 * 시장 항목(한국·미국 증시)·주제(사용자 주제·자동 토픽)를 stocks 표에 둔다(11단계 결정: 같은 표, 종류로 구분).
 * 주제는 켜진 증시마다 항목을 하나씩 두고 그 증시 기준으로 평가한다(둘 다 켜져 있으면 둘 다).
 */

export const MARKET_ITEMS: Record<
  Market,
  { code: string; name: string; exchange: string }
> = {
  KR: { code: "MARKET", name: "한국 증시", exchange: "KOSPI·KOSDAQ" },
  US: { code: "MARKET", name: "미국 증시", exchange: "S&P 500·NASDAQ" },
};
const TOPIC_NAME_MAX = 20;
/**
 * 새로 켜거나 되살린 시장 항목·사용자 주제 항목은 종목 등록과 같이(2026-10-21 결정) 트리를 만든 뒤
 * 다음 수집 회차에서 지난 24시간 기사까지 건다. 자동 토픽 항목은 보관한 제목으로 바로 건다(11단계 결정).
 */
const backfillFrom = (now: Date) =>
  new Date(now.getTime() - NEW_STOCK_BACKFILL_MS);
const MAX_TERM_LENGTH = 40;
const TOPIC_REASON = "토픽 검색어";

export type Stock = typeof stocks.$inferSelect;
export type Topic = typeof topics.$inferSelect;

function marketRow(db: Db, market: Market): Stock | undefined {
  return db
    .select()
    .from(stocks)
    .where(
      and(
        eq(stocks.kind, "market"),
        eq(stocks.market, market),
        eq(stocks.code, MARKET_ITEMS[market].code),
      ),
    )
    .get();
}

/** 켜진 증시 */
export function enabledMarkets(db: Db): Market[] {
  return MARKETS.filter((market) => {
    const row = marketRow(db, market);
    return row !== undefined && row.deletedAt === null;
  });
}

/**
 * 한국 증시는 켠 채로 시작한다(11단계 결정). 한 번도 만든 적이 없을 때만 만들고(사용자가 끈 것은 그대로),
 * 새로 만들었으면 키워드를 만들 항목 id를 돌려준다.
 */
export function ensureDefaultMarket(db: Db, now: Date): number | null {
  if (marketRow(db, "KR")) return null;
  return createMarketRow(db, "KR", now);
}

function createMarketRow(db: Db, market: Market, now: Date): number {
  const item = MARKET_ITEMS[market];
  return db
    .insert(stocks)
    .values({
      market,
      code: item.code,
      name: item.name,
      nameKo: item.name,
      exchange: item.exchange,
      userInput: item.name,
      kind: "market",
      keywordsStatus: "pending",
      backfillFrom: backfillFrom(now),
      createdAt: now,
    })
    .returning({ id: stocks.id })
    .get().id;
}

/**
 * 증시를 켜거나 끈다: 시장 항목을 만들거나 되살리거나 숨기고, 모든 주제·토픽의 항목을 켜진 증시에 맞춘다.
 * 키워드를 새로 만들 항목(시장 항목, 사용자 주제 항목)과 지난 기사를 걸 항목(자동 토픽 항목)을 돌려준다.
 */
export function setMarketEnabled(
  db: Db,
  market: Market,
  enabled: boolean,
  now: Date,
): { generate: number[]; created: number[] } {
  const generate: number[] = [];
  const created: number[] = [];
  db.transaction(() => {
    const row = marketRow(db, market);
    if (enabled) {
      if (!row) generate.push(createMarketRow(db, market, now));
      else if (row.deletedAt !== null) {
        db.update(stocks)
          .set({
            deletedAt: null,
            keywordsStatus: "pending",
            backfillFrom: backfillFrom(now),
          })
          .where(eq(stocks.id, row.id))
          .run();
        generate.push(row.id);
      }
    } else if (row && row.deletedAt === null) {
      db.update(stocks)
        .set({ deletedAt: now })
        .where(eq(stocks.id, row.id))
        .run();
    }
    for (const topic of liveTopics(db)) {
      for (const lens of syncTopicLenses(db, topic, now)) {
        if (topic.source === "user") generate.push(lens);
        else created.push(lens);
      }
    }
  });
  return { generate, created };
}

/** 지우지 않은 토픽(자동 토픽은 끝난 것도 포함, 3일 동안 화면에 남으므로) */
function liveTopics(db: Db): Topic[] {
  return db
    .select()
    .from(topics)
    .where(isNull(topics.deletedAt))
    .orderBy(asc(topics.id))
    .all();
}

/** 토픽의 항목(증시별) */
export function topicLenses(db: Db, topicId: number): Stock[] {
  return db
    .select()
    .from(stocks)
    .where(and(eq(stocks.kind, "topic"), eq(stocks.topicId, topicId)))
    .all();
}

/**
 * 토픽의 항목을 켜진 증시에 맞춘다: 없는 것은 만들고 숨긴 것은 되살리며, 꺼진 증시의 것은 숨긴다.
 * 새로 만들었거나 되살린 항목 id를 돌려준다. 자동 토픽 항목은 검색어 키워드도 맞춘다.
 */
export function syncTopicLenses(db: Db, topic: Topic, now: Date): number[] {
  const markets = new Set(enabledMarkets(db));
  const lenses = topicLenses(db, topic.id);
  const added: number[] = [];
  for (const market of MARKETS) {
    const lens = lenses.find((row) => row.market === market);
    if (markets.has(market)) {
      if (!lens) {
        const id = db
          .insert(stocks)
          .values({
            market,
            code: `TOPIC-${topic.id}`,
            name: topic.name,
            nameKo: topic.name,
            exchange: MARKET_ITEMS[market].name,
            userInput: topic.name,
            kind: "topic",
            topicId: topic.id,
            keywordsStatus: topic.source === "auto" ? "ready" : "pending",
            keywordsGeneratedAt: topic.source === "auto" ? now : null,
            backfillFrom: topic.source === "user" ? backfillFrom(now) : null,
            createdAt: now,
          })
          .returning({ id: stocks.id })
          .get().id;
        added.push(id);
        if (topic.source === "auto") setAutoTopicTerms(db, id, topic, now);
      } else if (lens.deletedAt !== null) {
        db.update(stocks)
          .set({
            deletedAt: null,
            ...(topic.source === "user"
              ? { keywordsStatus: "pending", backfillFrom: backfillFrom(now) }
              : {}),
          })
          .where(eq(stocks.id, lens.id))
          .run();
        added.push(lens.id);
        if (topic.source === "auto") setAutoTopicTerms(db, lens.id, topic, now);
      }
    } else if (lens && lens.deletedAt === null) {
      db.update(stocks)
        .set({ deletedAt: now })
        .where(eq(stocks.id, lens.id))
        .run();
    }
  }
  return added;
}

/** 자동 토픽 항목의 키워드 = 토픽 검색어(1단계, 시사). 토픽 갱신 때마다 바꾼다. */
export function setAutoTopicTerms(
  db: Db,
  lensId: number,
  topic: Pick<Topic, "name" | "summary" | "terms">,
  now: Date,
): void {
  db.transaction((tx) => {
    tx.delete(keywords).where(eq(keywords.stockId, lensId)).run();
    for (const term of topic.terms) {
      const text = term.replace(/\s+/g, " ").trim().slice(0, MAX_TERM_LENGTH);
      if (!text) continue;
      tx.insert(keywords)
        .values({
          stockId: lensId,
          level: 1,
          textKo: text,
          reason: topic.summary ?? TOPIC_REASON,
          path: `${topic.name}${PATH_SEPARATOR}${text}`,
          kind: "current",
          createdAt: now,
        })
        .run();
    }
    tx.update(stocks)
      .set({ name: topic.name, nameKo: topic.name, keywordsGeneratedAt: now })
      .where(eq(stocks.id, lensId))
      .run();
  });
}

export type AddTopicResult =
  | { ok: true; topicId: number; lensIds: number[] }
  | { ok: false; error: "invalid" | "exists" };

/** 사용자 주제를 넣는다(적은 그대로 2~20자, 11단계 결정). 키워드를 만들 항목 id를 돌려준다. */
export function addUserTopic(db: Db, input: string, now: Date): AddTopicResult {
  const name = input.replace(/\s+/g, " ").trim();
  if (name.length < 2 || name.length > TOPIC_NAME_MAX) {
    return { ok: false, error: "invalid" };
  }
  const key = name.replace(/\s+/g, "").toLowerCase();
  const exists = db
    .select({ name: topics.name })
    .from(topics)
    .where(and(eq(topics.source, "user"), isNull(topics.deletedAt)))
    .all()
    .some((topic) => topic.name.replace(/\s+/g, "").toLowerCase() === key);
  if (exists) return { ok: false, error: "exists" };
  return db.transaction(() => {
    const topic = db
      .insert(topics)
      .values({ source: "user", name, createdAt: now })
      .returning()
      .get();
    return {
      ok: true as const,
      topicId: topic.id,
      lensIds: syncTopicLenses(db, topic, now),
    };
  });
}

/** 사용자 주제를 지운다(항목은 숨기고 보존, D-11과 같은 방식) */
export function deleteUserTopic(db: Db, topicId: number, now: Date): boolean {
  const topic = db
    .select()
    .from(topics)
    .where(and(eq(topics.id, topicId), eq(topics.source, "user")))
    .get();
  if (!topic || topic.deletedAt) return false;
  db.transaction(() => {
    db.update(topics)
      .set({ deletedAt: now })
      .where(eq(topics.id, topicId))
      .run();
    db.update(stocks)
      .set({ deletedAt: now })
      .where(and(eq(stocks.topicId, topicId), isNull(stocks.deletedAt)))
      .run();
  });
  return true;
}

/** 사용자 주제 목록(지우지 않은 것) */
export function listUserTopics(db: Db): Topic[] {
  return db
    .select()
    .from(topics)
    .where(and(eq(topics.source, "user"), isNull(topics.deletedAt)))
    .orderBy(asc(topics.id))
    .all();
}

/**
 * 종목에 토픽 가지를 붙이거나 고친다(11단계): 1단계에 토픽 이름(이유·Jev 점수), 그 아래에 토픽 검색어.
 * 트리를 다시 만들어도 남고(topic_id), 토픽 갱신이 뗀다. 새로 붙였으면 true.
 */
export function attachTopicToStock(
  db: Db,
  stock: Pick<Stock, "id" | "name" | "nameKo">,
  topic: Pick<Topic, "id" | "name" | "terms">,
  link: { reason: string; relevance: number; magnitude: number },
  now: Date,
): boolean {
  const display = stock.nameKo ?? stock.name;
  const had = attachedRows(db, stock.id, topic.id).length > 0;
  db.transaction((tx) => {
    tx.delete(keywords)
      .where(
        and(eq(keywords.stockId, stock.id), eq(keywords.topicId, topic.id)),
      )
      .run();
    const themePath = `${display}${PATH_SEPARATOR}${topic.name}`;
    const themeId = tx
      .insert(keywords)
      .values({
        stockId: stock.id,
        level: 1,
        textKo: topic.name.slice(0, MAX_TERM_LENGTH),
        reason: link.reason,
        path: themePath,
        kind: "current",
        relevance: link.relevance,
        magnitude: link.magnitude,
        topicId: topic.id,
        createdAt: now,
      })
      .returning({ id: keywords.id })
      .get().id;
    for (const term of topic.terms) {
      const text = term.replace(/\s+/g, " ").trim().slice(0, MAX_TERM_LENGTH);
      if (!text) continue;
      tx.insert(keywords)
        .values({
          stockId: stock.id,
          level: 2,
          parentId: themeId,
          textKo: text,
          reason: TOPIC_REASON,
          path: `${themePath}${PATH_SEPARATOR}${text}`,
          kind: "current",
          topicId: topic.id,
          createdAt: now,
        })
        .run();
    }
  });
  return !had;
}

/** 종목에서 토픽 가지를 뗀다. 떼었으면 true */
export function detachTopicFromStock(
  db: Db,
  stockId: number,
  topicId: number,
): boolean {
  return (
    db
      .delete(keywords)
      .where(and(eq(keywords.stockId, stockId), eq(keywords.topicId, topicId)))
      .run().changes > 0
  );
}

function attachedRows(db: Db, stockId: number, topicId: number) {
  return db
    .select({ id: keywords.id })
    .from(keywords)
    .where(and(eq(keywords.stockId, stockId), eq(keywords.topicId, topicId)))
    .all();
}

/** 지금 종목마다 붙어 있는 토픽 */
export function attachedTopicsByStock(db: Db): Map<number, Set<number>> {
  const rows = db
    .select({ stockId: keywords.stockId, topicId: keywords.topicId })
    .from(keywords)
    .innerJoin(stocks, eq(stocks.id, keywords.stockId))
    .where(and(isNotNull(keywords.topicId), eq(stocks.kind, "stock")))
    .all();
  const map = new Map<number, Set<number>>();
  for (const row of rows) {
    if (row.topicId === null) continue;
    const set = map.get(row.stockId) ?? new Set<number>();
    set.add(row.topicId);
    map.set(row.stockId, set);
  }
  return map;
}

/** 활성 종목(종목 종류만): 토픽을 붙일 후보 */
export function activeStocks(db: Db): Stock[] {
  return db
    .select()
    .from(stocks)
    .where(and(eq(stocks.kind, "stock"), isNull(stocks.deletedAt)))
    .orderBy(asc(stocks.id))
    .all();
}

/** 자동 토픽(진행 중인 것) */
export function activeAutoTopics(db: Db): Topic[] {
  return db
    .select()
    .from(topics)
    .where(
      and(
        eq(topics.source, "auto"),
        eq(topics.status, "active"),
        isNull(topics.deletedAt),
      ),
    )
    .orderBy(asc(topics.id))
    .all();
}

/** 이 토픽들의 항목 id */
export function lensIdsOf(db: Db, topicIds: number[]): number[] {
  if (topicIds.length === 0) return [];
  return db
    .select({ id: stocks.id })
    .from(stocks)
    .where(
      and(
        eq(stocks.kind, "topic"),
        inArray(stocks.topicId, topicIds),
        isNull(stocks.deletedAt),
      ),
    )
    .all()
    .map((row) => row.id);
}
