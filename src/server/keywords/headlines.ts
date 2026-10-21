import { and, desc, eq, gte, inArray } from "drizzle-orm";
import type { Db } from "@/server/db/client";
import { articleMatches, articles, evaluations } from "@/server/db/schema";
import { readSetting } from "@/server/settings";

/** 키워드를 다시 만들 때 참고할 기사 기간과 개수(2026-10-21 결정) */
export const HEADLINE_DAYS = 7;
export const MAX_HEADLINES = 20;

const DAY_MS = 24 * 60 * 60 * 1000;

/**
 * 키워드를 만들 때 LLM에 함께 주는 그 종목의 최근 주요 기사 제목(SPEC FR-3, 2026-10-21 결정).
 * LLM은 오늘 뉴스를 모르므로 이것으로 요즘 이슈를 알려 준다. 종목 이름·Finnhub 직접으로 걸리고
 * 관련도가 표시 기준 이상인 최근 7일 기사를 최근순으로 최대 20개(같은 제목은 하나).
 * 키워드로 걸린 기사는 넣지 않는다(키워드가 스스로를 강화하지 않게).
 */
export function recentHeadlines(db: Db, stockId: number, now: Date): string[] {
  const rows = db
    .select({ title: articles.title })
    .from(articleMatches)
    .innerJoin(articles, eq(articles.id, articleMatches.articleId))
    .innerJoin(evaluations, eq(evaluations.matchId, articleMatches.id))
    .where(
      and(
        eq(articleMatches.stockId, stockId),
        inArray(articleMatches.matchKind, ["name", "direct"]),
        gte(evaluations.relevance, readSetting(db, "evaluation.minRelevance")),
        gte(
          articles.publishedAt,
          new Date(now.getTime() - HEADLINE_DAYS * DAY_MS),
        ),
      ),
    )
    .orderBy(desc(articles.publishedAt), desc(articles.id))
    .all();
  const titles: string[] = [];
  const seen = new Set<string>();
  for (const row of rows) {
    const title = row.title.replace(/\s+/g, " ").trim();
    if (!title || seen.has(title)) continue;
    seen.add(title);
    titles.push(title);
    if (titles.length >= MAX_HEADLINES) break;
  }
  return titles;
}
