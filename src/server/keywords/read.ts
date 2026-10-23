import { and, asc, eq, inArray, isNull, ne, or, sql } from "drizzle-orm";
import type { Db } from "@/server/db/client";
import { keywords, stocks, topics } from "@/server/db/schema";
import { readSetting } from "@/server/settings";
import { clampDepth } from "./tree";

export type Keyword = typeof keywords.$inferSelect;

/** 종목별 키워드(단계·생성 순서대로). web은 읽기만 한다. */
export function listKeywordsByStock(
  db: Db,
  stockIds: number[],
): Map<number, Keyword[]> {
  const grouped = new Map<number, Keyword[]>(stockIds.map((id) => [id, []]));
  if (stockIds.length === 0) return grouped;
  const rows = db
    .select()
    .from(keywords)
    .where(inArray(keywords.stockId, stockIds))
    .orderBy(asc(keywords.level), asc(keywords.id))
    .all();
  for (const row of rows) grouped.get(row.stockId)?.push(row);
  return grouped;
}

/**
 * 저장한 깊이와 다른 깊이로 키워드를 만든 종목(키워드가 없는 종목 포함). 만드는 중·대기 중인 종목은 뺀다.
 * 설정 화면의 "지금 다시 만들기"가 이 종목들을 다시 만든다(2026-10-21 결정).
 * 11단계: 트리를 만들지 않는 자동 토픽 항목은 빼고, 토픽 갱신이 붙인 가지는 깊이에 넣지 않는다.
 */
export function stocksWithOtherDepth(db: Db): number[] {
  const depth = clampDepth(readSetting(db, "keywords.depth"));
  return db
    .select({
      id: stocks.id,
      status: stocks.keywordsStatus,
      deepest: sql<number | null>`max(${keywords.level})`,
    })
    .from(stocks)
    .leftJoin(
      keywords,
      and(eq(keywords.stockId, stocks.id), isNull(keywords.topicId)),
    )
    .leftJoin(topics, eq(topics.id, stocks.topicId))
    .where(
      and(
        isNull(stocks.deletedAt),
        or(isNull(topics.source), ne(topics.source, "auto")),
      ),
    )
    .groupBy(stocks.id)
    .orderBy(asc(stocks.id))
    .all()
    .filter(
      (row) =>
        row.status !== "pending" &&
        row.status !== "generating" &&
        (row.deepest ?? 0) !== depth,
    )
    .map((row) => row.id);
}
