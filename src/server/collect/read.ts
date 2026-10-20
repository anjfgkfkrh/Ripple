import { count, desc } from "drizzle-orm";
import type { Db } from "@/server/db/client";
import { articleMatches, articles, collectionRuns } from "@/server/db/schema";

export type CollectionRun = typeof collectionRuns.$inferSelect;

/** 가장 최근 수집 회차. web은 읽기만 한다. */
export function latestRun(db: Db): CollectionRun | undefined {
  return db
    .select()
    .from(collectionRuns)
    .orderBy(desc(collectionRuns.id))
    .limit(1)
    .get();
}

/** 저장된 기사 수와 기사–종목 연결 수 */
export function collectionTotals(db: Db): {
  articles: number;
  matches: number;
} {
  return {
    articles: db.select({ n: count() }).from(articles).get()?.n ?? 0,
    matches: db.select({ n: count() }).from(articleMatches).get()?.n ?? 0,
  };
}
