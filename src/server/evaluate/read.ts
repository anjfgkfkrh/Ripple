import { and, count, eq, gte, isNull } from "drizzle-orm";
import type { Db } from "@/server/db/client";
import {
  articleMatches,
  articles,
  evaluations,
  stocks,
} from "@/server/db/schema";
import { readSetting } from "@/server/settings";
import { EVALUATION_WINDOW_MS } from "./run";

/** 평가한 연결 수와 표시 기준(설정 값) 이상인 수. web은 읽기만 한다. */
export function evaluationTotals(db: Db): {
  evaluated: number;
  relevant: number;
  minRelevance: number;
} {
  const minRelevance = readSetting(db, "evaluation.minRelevance");
  return {
    evaluated: db.select({ n: count() }).from(evaluations).get()?.n ?? 0,
    relevant:
      db
        .select({ n: count() })
        .from(evaluations)
        .where(gte(evaluations.relevance, minRelevance))
        .get()?.n ?? 0,
    minRelevance,
  };
}

/**
 * 아직 평가하지 않은 최근 7일 기사–종목 연결 수(삭제한 종목 제외, D-05와 같은 범위).
 * 키가 없거나, 예산을 다 썼거나, 회차 한도에 밀리면 쌓인다.
 */
export function countPendingEvaluations(
  db: Db,
  now: Date = new Date(),
): number {
  return (
    db
      .select({ n: count() })
      .from(articleMatches)
      .innerJoin(articles, eq(articles.id, articleMatches.articleId))
      .innerJoin(stocks, eq(stocks.id, articleMatches.stockId))
      .leftJoin(evaluations, eq(evaluations.matchId, articleMatches.id))
      .where(
        and(
          isNull(evaluations.id),
          isNull(stocks.deletedAt),
          gte(
            articles.publishedAt,
            new Date(now.getTime() - EVALUATION_WINDOW_MS),
          ),
        ),
      )
      .get()?.n ?? 0
  );
}
