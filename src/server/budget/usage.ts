import { gte, sql } from "drizzle-orm";
import {
  LlmError,
  type LlmClient,
  type LlmUsage,
  type StructuredRequest,
  type StructuredResult,
} from "@/server/ai/llm/types";
import type { Db } from "@/server/db/client";
import { usageLogs } from "@/server/db/schema";
import { readSetting } from "@/server/settings";
import { startOfSeoulMonth } from "@/server/time";

export class BudgetExceededError extends Error {
  constructor(
    readonly spentUsd: number,
    readonly limitUsd: number,
  ) {
    super(
      `이번 달 AI 예산을 모두 썼습니다(사용 $${spentUsd.toFixed(4)} / 한도 $${limitUsd}).`,
    );
    this.name = "BudgetExceededError";
  }
}

/** 이번 달(한국 시간) AI 사용액 합계, 달러 */
export function monthSpendUsd(db: Db, now: Date = new Date()): number {
  const row = db
    .select({ total: sql<number>`coalesce(sum(${usageLogs.costUsd}), 0)` })
    .from(usageLogs)
    .where(gte(usageLogs.at, startOfSeoulMonth(now)))
    .get();
  return Number(row?.total ?? 0);
}

export function recordUsage(
  db: Db,
  entry: {
    provider: "openrouter" | "typesafe";
    model: string;
    purpose: string;
    usage: LlmUsage;
    at?: Date;
  },
): void {
  db.insert(usageLogs)
    .values({
      at: entry.at ?? new Date(),
      provider: entry.provider,
      model: entry.model,
      purpose: entry.purpose,
      inputTokens: entry.usage.inputTokens,
      outputTokens: entry.usage.outputTokens,
      costUsd: entry.usage.costUsd,
    })
    .run();
}

/** 이번 달 사용액이 한도에 닿았으면 BudgetExceededError. AI 호출 전에 부른다(CLAUDE.md). */
export function assertWithinBudget(db: Db, now: Date = new Date()): void {
  const limitUsd = readSetting(db, "budget.monthlyUsd");
  const spentUsd = monthSpendUsd(db, now);
  if (spentUsd >= limitUsd) throw new BudgetExceededError(spentUsd, limitUsd);
}

/**
 * AI 호출은 반드시 이 함수를 거친다(CLAUDE.md): 월 한도 검사 → 호출 → 사용량 기록.
 * 응답을 받은 뒤 형식이 틀려 실패해도 청구된 비용은 기록한다.
 */
export async function generateWithBudget<T>(
  db: Db,
  llm: LlmClient,
  request: StructuredRequest<T>,
  options: { purpose: string; now?: () => Date },
): Promise<StructuredResult<T>> {
  const now = options.now ?? (() => new Date());
  assertWithinBudget(db, now());

  try {
    const result = await llm.generateObject(request);
    recordUsage(db, {
      provider: llm.provider,
      model: result.model,
      purpose: options.purpose,
      usage: result.usage,
      at: now(),
    });
    return result;
  } catch (error) {
    if (error instanceof LlmError && error.usage) {
      recordUsage(db, {
        provider: llm.provider,
        model: request.model,
        purpose: options.purpose,
        usage: error.usage,
        at: now(),
      });
    }
    throw error;
  }
}
