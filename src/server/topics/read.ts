import { and, asc, eq, inArray, isNull } from "drizzle-orm";
import type { Db } from "@/server/db/client";
import { MARKETS, stocks, topics, type Market } from "@/server/db/schema";
import { listUserTopics, MARKET_ITEMS, type Stock, type Topic } from "./store";

/** 시장 항목(한국·미국 증시)과 켜져 있으면 그 항목. web은 읽기만 한다(11단계). */
export type MarketItem = { market: Market; name: string; stock: Stock | null };

export function listMarketItems(db: Db): MarketItem[] {
  return MARKETS.map((market) => {
    const row = db
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
    return {
      market,
      name: MARKET_ITEMS[market].name,
      stock: row && row.deletedAt === null ? row : null,
    };
  });
}

/** 사용자 주제와 켜진 증시마다 하나씩인 항목(한국 먼저) */
export type UserTopicItem = { topic: Topic; lenses: Stock[] };

export function listUserTopicItems(db: Db): UserTopicItem[] {
  const list = listUserTopics(db);
  if (list.length === 0) return [];
  const lenses = db
    .select()
    .from(stocks)
    .where(
      and(
        eq(stocks.kind, "topic"),
        inArray(
          stocks.topicId,
          list.map((topic) => topic.id),
        ),
        isNull(stocks.deletedAt),
      ),
    )
    .orderBy(asc(stocks.market), asc(stocks.id))
    .all();
  return list.map((topic) => ({
    topic,
    lenses: lenses.filter((lens) => lens.topicId === topic.id),
  }));
}

/** 주소의 topic 값으로 받을 수 있는 토픽: 지우지 않은 사용자 주제와 자동 토픽(끝난 것 포함) */
export function filterableTopicIds(db: Db): number[] {
  return db
    .select({ id: topics.id })
    .from(topics)
    .where(isNull(topics.deletedAt))
    .all()
    .map((row) => row.id);
}

/** 토픽 이름(필터 안내용) */
export function topicName(db: Db, topicId: number): string | null {
  return (
    db
      .select({ name: topics.name })
      .from(topics)
      .where(eq(topics.id, topicId))
      .get()?.name ?? null
  );
}
