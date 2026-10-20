import { asc, inArray } from "drizzle-orm";
import type { Db } from "@/server/db/client";
import { keywords } from "@/server/db/schema";

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
