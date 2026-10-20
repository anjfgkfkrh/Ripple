import { count, gte } from "drizzle-orm";
import type { Db } from "@/server/db/client";
import { evaluations } from "@/server/db/schema";
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
