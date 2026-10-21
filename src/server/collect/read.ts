import { count, desc } from "drizzle-orm";
import type { Db } from "@/server/db/client";
import {
  articleMatches,
  articles,
  collectionRuns,
  type SourceResult,
} from "@/server/db/schema";

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

export type FailureGroup = { reason: string; labels: string[] };

/**
 * 실패한 소스를 같은 이유끼리 묶는다(많은 것 먼저). 회차 전체가 실패해도(인터넷 끊김 등)
 * 상태 화면이 소스 수만큼 길어지지 않게 한다(2026-10-21 결정).
 */
export function groupFailures(sources: SourceResult[]): FailureGroup[] {
  const byReason = new Map<string, string[]>();
  for (const source of sources) {
    if (!source.error) continue;
    const labels = byReason.get(source.error) ?? [];
    labels.push(source.label);
    byReason.set(source.error, labels);
  }
  return [...byReason]
    .map(([reason, labels]) => ({ reason, labels }))
    .sort((a, b) => b.labels.length - a.labels.length);
}
