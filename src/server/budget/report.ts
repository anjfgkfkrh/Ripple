import { desc, gte, sql } from "drizzle-orm";
import type { Db } from "@/server/db/client";
import { usageLogs } from "@/server/db/schema";
import { readSetting } from "@/server/settings";
import { startOfSeoulMonth } from "@/server/time";
import { monthSpendUsd } from "./usage";

/** 이만큼 쓰면 미리 알린다(D-07, 2026-10-21 결정). */
export const BUDGET_WARN_RATIO = 0.8;

/** ok: 여유, warn: 80% 넘음, over: 한도에 닿아 AI 호출을 멈춤(D-07) */
export type BudgetLevel = "ok" | "warn" | "over";

export function budgetLevel(spentUsd: number, limitUsd: number): BudgetLevel {
  if (spentUsd >= limitUsd) return "over";
  if (spentUsd >= limitUsd * BUDGET_WARN_RATIO) return "warn";
  return "ok";
}

/** 사용량 기록의 용도(purpose)를 화면 이름으로 */
export const PURPOSE_LABEL: Record<string, string> = {
  stock_match: "종목 찾기",
  keyword_generation: "키워드 만들기",
  market_issues: "시장 이슈 정리",
  article_evaluation: "기사 평가",
  keyword_generation_check: "품질 확인(개발 중 직접 호출)",
  market_issues_check: "품질 확인(개발 중 직접 호출)",
  article_evaluation_check: "품질 확인(개발 중 직접 호출)",
  model_compare: "품질 확인(개발 중 직접 호출)",
};

export type UsageRow = {
  provider: "openrouter" | "typesafe";
  purpose: string;
  label: string;
  calls: number;
  costUsd: number;
};

/** 이번 달(한국 시간) 제공자·용도별 호출 수와 비용, 한도(SPEC FR-8, 2026-10-21 결정) */
export function monthUsageReport(
  db: Db,
  now: Date = new Date(),
): {
  rows: UsageRow[];
  totalUsd: number;
  limitUsd: number;
  level: BudgetLevel;
} {
  const rows = db
    .select({
      provider: usageLogs.provider,
      purpose: usageLogs.purpose,
      calls: sql<number>`count(*)`,
      costUsd: sql<number>`coalesce(sum(${usageLogs.costUsd}), 0)`,
    })
    .from(usageLogs)
    .where(gte(usageLogs.at, startOfSeoulMonth(now)))
    .groupBy(usageLogs.provider, usageLogs.purpose)
    .orderBy(desc(sql`sum(${usageLogs.costUsd})`))
    .all();
  // 화면 이름이 같은 용도(개발 중 품질 확인 여러 가지)는 제공자마다 한 줄로 합친다.
  const merged = new Map<string, UsageRow>();
  for (const row of rows) {
    const label = PURPOSE_LABEL[row.purpose] ?? row.purpose;
    const key = `${row.provider}:${label}`;
    const prev = merged.get(key);
    merged.set(key, {
      provider: row.provider,
      purpose: prev?.purpose ?? row.purpose,
      label,
      calls: (prev?.calls ?? 0) + Number(row.calls),
      costUsd: (prev?.costUsd ?? 0) + Number(row.costUsd),
    });
  }
  const totalUsd = monthSpendUsd(db, now);
  const limitUsd = readSetting(db, "budget.monthlyUsd");
  return {
    rows: [...merged.values()].sort((a, b) => b.costUsd - a.costUsd),
    totalUsd,
    limitUsd,
    level: budgetLevel(totalUsd, limitUsd),
  };
}
