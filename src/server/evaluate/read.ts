import { and, count, eq, gte, isNull } from "drizzle-orm";
import type { Db } from "@/server/db/client";
import { articleMatches, evaluations, stocks } from "@/server/db/schema";
import { readSetting } from "@/server/settings";

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

/** 아직 평가하지 않은 기사–종목 연결 수(삭제한 종목 제외). 키가 없거나 예산을 다 쓰면 쌓인다. */
export function countPendingEvaluations(db: Db): number {
  return (
    db
      .select({ n: count() })
      .from(articleMatches)
      .innerJoin(stocks, eq(stocks.id, articleMatches.stockId))
      .leftJoin(evaluations, eq(evaluations.matchId, articleMatches.id))
      .where(and(isNull(evaluations.id), isNull(stocks.deletedAt)))
      .get()?.n ?? 0
  );
}
