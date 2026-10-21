import { and, desc, eq, gte, isNull, sql } from "drizzle-orm";
import {
  DeciderError,
  type Decider,
  type DecisionInput,
} from "@/server/ai/decider/types";
import {
  assertWithinBudget,
  BudgetExceededError,
  recordUsage,
} from "@/server/budget/usage";
import type { Db } from "@/server/db/client";
import {
  articleMatches,
  articles,
  evaluations,
  stocks,
  type EvaluationSummary,
} from "@/server/db/schema";
import { logError } from "@/server/log";
import { readSetting } from "@/server/settings";

/** 연달아 이만큼 실패하면(TypeSafe 장애 등) 이번 회차 평가를 멈춘다. 호출마다 재시도·제한 시간을 다 기다리지 않게. */
export const MAX_CONSECUTIVE_FAILURES = 3;
/** 이보다 오래된 기사는 평가하지 않는다(D-05: 한도·예산에 밀린 옛 기사보다 요즘 기사를 먼저). */
export const EVALUATION_WINDOW_MS = 7 * 24 * 60 * 60 * 1000;

type Pending = {
  matchId: number;
  articleId: number;
  stockId: number;
  input: DecisionInput;
};

/**
 * 아직 평가하지 않은 최근 7일 기사–종목 연결(D-05). 화면에 나올 가능성이 큰 종목 이름·Finnhub 직접 연결을
 * 먼저, 그다음 최근 기사부터(실측: 키워드 연결은 90건 중 1건만 6점 이상, SPEC 11장).
 */
export function pendingEvaluations(db: Db, now: Date = new Date()): Pending[] {
  const rows = db
    .select({
      matchId: articleMatches.id,
      articleId: articles.id,
      stockId: stocks.id,
      kind: articleMatches.matchKind,
      path: articleMatches.keywordPath,
      reason: articleMatches.keywordReason,
      title: articles.title,
      summary: articles.summary,
      name: stocks.name,
      nameKo: stocks.nameKo,
      market: stocks.market,
      exchange: stocks.exchange,
      code: stocks.code,
    })
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
    .orderBy(
      sql`case when ${articleMatches.matchKind} = 'keyword' then 1 else 0 end`,
      desc(articles.publishedAt),
      desc(articleMatches.id),
    )
    .all();
  return rows.map((r) => ({
    matchId: r.matchId,
    articleId: r.articleId,
    stockId: r.stockId,
    input: {
      stock: {
        name: r.nameKo ?? r.name,
        officialName: r.nameKo && r.nameKo !== r.name ? r.name : null,
        market: r.market,
        exchange: r.exchange,
        code: r.code,
      },
      link: { kind: r.kind, path: r.path, reason: r.reason },
      article: { title: r.title, summary: r.summary },
    },
  }));
}

/**
 * 평가하지 않은 연결을 Jev로 평가한다(SPEC FR-5). 호출마다 월 예산을 먼저 보고(CLAUDE.md),
 * 쓴 비용을 기록한다(D-06: 입력 토큰 × 단가). 하나씩 차례로 불러 예산 검사에 앞선 호출이 모두 들어가게 한다.
 * 예산을 다 쓰거나, 키가 없거나 맞지 않거나, 연달아 실패하면 멈추고 나머지는 남긴다
 * (수집은 계속, D-07 최종은 8단계). 실패한 것은 다음 회차에 다시 평가한다.
 */
export async function evaluatePending(deps: {
  db: Db;
  decider: Decider | null;
  now?: () => Date;
}): Promise<EvaluationSummary> {
  const { db } = deps;
  const now = deps.now ?? (() => new Date());
  const started = Date.now();
  const pending = pendingEvaluations(db, now());
  const summary: EvaluationSummary = {
    evaluated: 0,
    relevant: 0,
    failed: 0,
    remaining: 0,
    costUsd: 0,
    ms: 0,
  };
  if (!deps.decider) {
    return {
      ...summary,
      remaining: pending.length,
      note: "TYPESAFE_API_KEY가 없어 평가하지 않았습니다.",
    };
  }
  const decider = deps.decider;
  const model = readSetting(db, "jev.model");
  const price = readSetting(db, "jev.usdPerMillionInputTokens");
  const minRelevance = readSetting(db, "evaluation.minRelevance");
  // 회차당 한도(D-03). 넘는 것은 다음 회차로 넘긴다(D-05).
  const maxPerRun = Math.max(1, readSetting(db, "evaluation.maxPerRun"));

  let note: string | undefined;
  let failedInRow = 0;
  for (const item of pending.slice(0, maxPerRun)) {
    try {
      assertWithinBudget(db, now());
    } catch (error) {
      if (!(error instanceof BudgetExceededError)) throw error;
      note = "이번 달 AI 예산을 모두 써서 평가를 멈췄습니다.";
      break;
    }
    try {
      const decision = await decider.decide(item.input, model);
      const costUsd = (decision.usage.inputTokens * price) / 1_000_000;
      const at = now();
      recordUsage(db, {
        provider: "typesafe",
        model: decision.model,
        purpose: "article_evaluation",
        usage: { ...decision.usage, costUsd },
        at,
      });
      db.insert(evaluations)
        .values({
          matchId: item.matchId,
          articleId: item.articleId,
          stockId: item.stockId,
          relevance: decision.relevance,
          relevanceConfidence: decision.relevanceConfidence,
          impact: decision.impact,
          impactConfidence: decision.impactConfidence,
          impactProbabilities: decision.impactProbabilities,
          model: decision.model,
          evaluatedAt: at,
        })
        .onConflictDoNothing()
        .run();
      summary.evaluated += 1;
      summary.costUsd += costUsd;
      if (decision.relevance >= minRelevance) summary.relevant += 1;
      failedInRow = 0;
    } catch (error) {
      if (error instanceof DeciderError && error.fatal) {
        note = error.message;
        break;
      }
      summary.failed += 1;
      failedInRow += 1;
      logError(
        `기사 평가 실패: 연결 ${item.matchId}`,
        error instanceof Error ? error.message : error,
      );
      if (failedInRow >= MAX_CONSECUTIVE_FAILURES) {
        note = `Jev 호출이 ${MAX_CONSECUTIVE_FAILURES}번 연달아 실패해 평가를 멈췄습니다. 다음 회차에 다시 합니다.`;
        break;
      }
    }
  }

  summary.remaining = pending.length - summary.evaluated - summary.failed;
  summary.ms = Date.now() - started;
  if (!note && pending.length > maxPerRun) {
    note = `회차 한도 ${maxPerRun}건까지 평가했습니다. 남은 ${summary.remaining}건은 다음 회차에 평가합니다.`;
  }
  if (note) summary.note = note;
  return summary;
}
