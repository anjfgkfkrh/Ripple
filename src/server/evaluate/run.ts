import { and, desc, eq, gte, isNull, lte, sql } from "drizzle-orm";
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
  topics,
  type EvaluationSummary,
} from "@/server/db/schema";
import { logError } from "@/server/log";
import { readSetting } from "@/server/settings";
import {
  isSameNews,
  SAME_NEWS_WINDOW_MS,
  titleFeatures,
} from "@/server/similar-title";

/** 연달아 이만큼 실패하면(TypeSafe 장애 등) 이번 회차 평가를 멈춘다. 호출마다 재시도·제한 시간을 다 기다리지 않게. */
export const MAX_CONSECUTIVE_FAILURES = 3;
/** 이보다 오래된 기사는 평가하지 않는다(D-05: 한도·예산에 밀린 옛 기사보다 요즘 기사를 먼저). */
export const EVALUATION_WINDOW_MS = 7 * 24 * 60 * 60 * 1000;

type Pending = {
  matchId: number;
  articleId: number;
  stockId: number;
  publishedAt: Date;
  input: DecisionInput;
};

/**
 * 아직 평가하지 않은 최근 7일 기사–항목 연결(D-05). 화면에 나올 가능성이 큰 종목 이름·Finnhub 직접 연결을
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
      publishedAt: articles.publishedAt,
      itemKind: stocks.kind,
      name: stocks.name,
      nameKo: stocks.nameKo,
      market: stocks.market,
      exchange: stocks.exchange,
      code: stocks.code,
      topicSummary: topics.summary,
    })
    .from(articleMatches)
    .innerJoin(articles, eq(articles.id, articleMatches.articleId))
    .innerJoin(stocks, eq(stocks.id, articleMatches.stockId))
    .leftJoin(topics, eq(topics.id, stocks.topicId))
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
    publishedAt: r.publishedAt,
    input: {
      stock: {
        kind: r.itemKind,
        name: r.nameKo ?? r.name,
        officialName: r.nameKo && r.nameKo !== r.name ? r.name : null,
        market: r.market,
        exchange: r.exchange,
        code: r.code,
        summary: r.topicSummary,
      },
      link: { kind: r.kind, path: r.path, reason: r.reason },
      article: { title: r.title, summary: r.summary },
    },
  }));
}

type Evaluation = typeof evaluations.$inferSelect;

/**
 * 같은 항목에 걸린 같은 소식(비슷한 제목, 발행 24시간 안)의 앞선 평가(11단계 결정: 같은 소식은 한 번만 Jev에 묻는다).
 * 다른 평가를 그대로 쓴 평가는 고르지 않는다(처음 평가한 것을 쓴다).
 */
function findSameNewsEvaluation(db: Db, item: Pending): Evaluation | null {
  const at = item.publishedAt.getTime();
  const candidates = db
    .select({ evaluation: evaluations, title: articles.title })
    .from(evaluations)
    .innerJoin(articles, eq(articles.id, evaluations.articleId))
    .where(
      and(
        eq(evaluations.stockId, item.stockId),
        isNull(evaluations.copiedFrom),
        gte(articles.publishedAt, new Date(at - SAME_NEWS_WINDOW_MS)),
        lte(articles.publishedAt, new Date(at + SAME_NEWS_WINDOW_MS)),
      ),
    )
    .all();
  if (candidates.length === 0) return null;
  const mine = titleFeatures(item.input.article.title);
  return (
    candidates.find((candidate) =>
      isSameNews(mine, titleFeatures(candidate.title)),
    )?.evaluation ?? null
  );
}

/** 표시 기준(관련도·영향도 둘 다, 11단계) 이상인지 */
export function meetsDisplayRule(
  db: Db,
  relevance: number,
  magnitude: number | null,
): boolean {
  if (relevance < readSetting(db, "evaluation.minRelevance")) return false;
  // 11단계 전에 평가한 기사(영향도 없음)는 관련도만 본다.
  return (
    magnitude === null ||
    magnitude >= readSetting(db, "evaluation.minMagnitude")
  );
}

/**
 * 평가하지 않은 연결을 Jev로 평가한다(SPEC FR-5, 11단계: 관련도·영향도·호재/악재). 호출마다 월 예산을 먼저 보고
 * (CLAUDE.md), 쓴 비용을 항목과 함께 기록한다(D-06: 입력 토큰 × 단가). 하나씩 차례로 불러 예산 검사에 앞선 호출이
 * 모두 들어가게 한다. 같은 항목의 같은 소식(비슷한 제목)은 앞선 평가를 그대로 쓰고 Jev를 부르지 않는다(회차 한도에도 세지 않음).
 * 예산을 다 쓰거나, 키가 없거나 맞지 않거나, 연달아 실패하면 멈추고 나머지는 남긴다. 실패한 것은 다음 회차에 다시 평가한다.
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
    copied: 0,
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
  // 회차당 한도(D-03). 넘는 것은 다음 회차로 넘긴다(D-05). 같은 소식 복사는 세지 않는다.
  const maxPerRun = Math.max(1, readSetting(db, "evaluation.maxPerRun"));

  let note: string | undefined;
  let failedInRow = 0;
  let calls = 0;
  let stoppedAt = pending.length;
  for (const [index, item] of pending.entries()) {
    const same = findSameNewsEvaluation(db, item);
    if (same) {
      db.insert(evaluations)
        .values({
          matchId: item.matchId,
          articleId: item.articleId,
          stockId: item.stockId,
          relevance: same.relevance,
          relevanceConfidence: same.relevanceConfidence,
          magnitude: same.magnitude,
          magnitudeConfidence: same.magnitudeConfidence,
          impact: same.impact,
          impactConfidence: same.impactConfidence,
          impactProbabilities: same.impactProbabilities,
          model: same.model,
          copiedFrom: same.id,
          evaluatedAt: now(),
        })
        .onConflictDoNothing()
        .run();
      summary.copied! += 1;
      if (meetsDisplayRule(db, same.relevance, same.magnitude))
        summary.relevant += 1;
      continue;
    }
    if (calls >= maxPerRun) {
      stoppedAt = index;
      note = `회차 한도 ${maxPerRun}건까지 평가했습니다.`;
      break;
    }
    try {
      assertWithinBudget(db, now());
    } catch (error) {
      if (!(error instanceof BudgetExceededError)) throw error;
      note = "이번 달 AI 예산을 모두 써서 평가를 멈췄습니다.";
      stoppedAt = index;
      break;
    }
    calls += 1;
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
        stockId: item.stockId,
      });
      db.insert(evaluations)
        .values({
          matchId: item.matchId,
          articleId: item.articleId,
          stockId: item.stockId,
          relevance: decision.relevance,
          relevanceConfidence: decision.relevanceConfidence,
          magnitude: decision.magnitude,
          magnitudeConfidence: decision.magnitudeConfidence,
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
      if (meetsDisplayRule(db, decision.relevance, decision.magnitude))
        summary.relevant += 1;
      failedInRow = 0;
    } catch (error) {
      if (error instanceof DeciderError && error.fatal) {
        note = error.message;
        stoppedAt = index;
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
        stoppedAt = index + 1;
        break;
      }
    }
  }

  summary.remaining = pending.length - stoppedAt;
  summary.ms = Date.now() - started;
  if (note?.startsWith("회차 한도")) {
    note = `${note} 남은 ${summary.remaining}건은 다음 회차에 평가합니다.`;
  }
  if (note) summary.note = note;
  return summary;
}
