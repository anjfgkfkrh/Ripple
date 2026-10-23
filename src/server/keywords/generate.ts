import { and, asc, eq, inArray, isNull } from "drizzle-orm";
import {
  DeciderError,
  type Decider,
  type JudgeItem,
} from "@/server/ai/decider/types";
import { LlmError, type LlmClient } from "@/server/ai/llm/types";
import {
  BudgetExceededError,
  generateWithBudget,
  rateWithBudget,
} from "@/server/budget/usage";
import type { Db } from "@/server/db/client";
import {
  keywords,
  stocks,
  topics,
  type KeywordStatus,
} from "@/server/db/schema";
import { log, logError } from "@/server/log";
import { readSetting } from "@/server/settings";
import { recentHeadlines } from "./headlines";
import {
  buildKeywordMessages,
  buildKeywordTree,
  clampDepth,
  countRawKeywords,
  keywordTreeSchema,
  MAX_KEPT_KEYWORDS,
  pruneKeywordTree,
  type KeywordNode,
} from "./tree";

/** LLM은 답했지만 규칙에 맞는 키워드가 하나도 없을 때 */
class NoUsableKeywordsError extends Error {}

/**
 * 2026-10-22 실측: 깊이 3에 14~16초(10단계). 11단계에서 후보를 단계마다 5~8개로 늘리자
 * 한국 증시 트리가 8,000토큰에 34초 걸려 잘렸다. 깊이 4와 느린 때를 생각해 넉넉히 둔다.
 */
const KEYWORD_TIMEOUT_MS = 120_000;
/** 추론 토큰과 함께 쓴다(11단계 실측: 후보를 넉넉히 받으면 8,000토큰을 넘을 수 있음) */
const KEYWORD_MAX_TOKENS = 16_000;
/** 규칙에 안 맞는 답을 볼 때 로그에 남기는 LLM 원문 길이 */
const RAW_LOG_MAX_CHARS = 6000;
/** Jev 평가가 연달아 이만큼 실패하면 평가를 멈추고 후보를 자르지 않는다. */
const MAX_RATING_FAILURES = 3;

/** 실패 이유(error)는 화면에 "키워드를 만들지 못했습니다." 뒤에 붙여 보여 준다. */
export type GenerateResult =
  | {
      ok: true;
      /** 남긴 키워드 수 */
      count: number;
      depth: number;
      /** LLM이 줬지만 규칙(부모·중복 등)에 맞지 않아 버린 후보 수 */
      dropped: number;
      /** 검증을 통과한 후보 수와, 그중 Jev가 평가한 수(11단계) */
      candidates: number;
      rated: number;
      /** Jev로 자르지 못한 이유(키 없음·예산 등). 그때는 후보를 상한까지 그대로 남긴다 */
      unratedReason?: string;
      /** 참고로 넣은 최근 기사 제목 수 */
      headlines: number;
    }
  | { ok: false; error: string };

/**
 * 항목(종목·시장·사용자 주제) 하나의 키워드 트리를 만든다(SPEC FR-3, 11단계).
 * LLM이 후보를 넉넉히 내고(호출 1번), Jev가 후보마다 관련도·영향도를 매겨 설정 기준을 넘는 것만 남긴다.
 * 성공하면 이전 키워드를 통째로 바꾸되(D-16) 토픽 갱신이 붙인 가지는 남기고,
 * 실패하면 이전 키워드를 그대로 두고 이유를 남긴다. 자동 토픽 항목은 토픽 갱신이 검색어를 정하므로 만들지 않는다.
 */
export async function generateKeywordsForStock(
  db: Db,
  llm: LlmClient,
  stockId: number,
  options: { now?: () => Date; decider?: Decider | null } = {},
): Promise<GenerateResult> {
  const now = options.now ?? (() => new Date());
  const row = db
    .select({ stock: stocks, topic: topics })
    .from(stocks)
    .leftJoin(topics, eq(topics.id, stocks.topicId))
    .where(eq(stocks.id, stockId))
    .get();
  const stock = row?.stock;
  if (!row || !stock || stock.deletedAt) {
    return { ok: false, error: "종목이 없거나 삭제됐습니다." };
  }
  if (row.topic?.source === "auto") {
    return { ok: false, error: "자동 토픽은 토픽 갱신이 검색어를 정합니다." };
  }

  setKeywordStatus(db, stockId, "generating");
  const depth = clampDepth(readSetting(db, "keywords.depth"));
  const headlines =
    stock.kind === "stock" ? recentHeadlines(db, stockId, now()) : [];
  try {
    const { value } = await generateWithBudget(
      db,
      llm,
      {
        model: readSetting(db, "llm.model"),
        messages: buildKeywordMessages(stock, depth, { headlines }),
        schema: keywordTreeSchema,
        schemaName: "keyword_tree",
        maxTokens: KEYWORD_MAX_TOKENS,
        timeoutMs: KEYWORD_TIMEOUT_MS,
      },
      { purpose: "keyword_generation", now, stockId },
    );
    const candidates = buildKeywordTree(stock, value, depth);
    const dropped = countRawKeywords(value) - candidates.length;
    const deepest = Math.max(0, ...candidates.map((node) => node.level));
    if (dropped > 0 || deepest < depth) {
      // 규칙에 맞지 않는 답(단계 누락 등)의 원인을 볼 수 있게 원문을 남긴다(10단계 결정).
      log(
        `키워드 LLM 원문(종목 id ${stockId}, 버림 ${dropped}개, 깊이 ${deepest}/${depth}): ${JSON.stringify(value).slice(0, RAW_LOG_MAX_CHARS)}`,
      );
    }
    if (candidates.length === 0) {
      throw new NoUsableKeywordsError();
    }

    const rating = await rateCandidates(db, options.decider ?? null, {
      item: judgeItem(stock, row.topic?.summary ?? null),
      nodes: candidates,
      stockId,
      now,
    });
    const kept = rating.stopped
      ? // Jev로 평가하지 못하면 자르지 않고 상한까지 남긴다(단계 순서라 부모가 먼저 남는다).
        candidates.slice(0, MAX_KEPT_KEYWORDS)
      : pruneKeywordTree(candidates, {
          minRelevance: readSetting(db, "keywords.minRelevance"),
          minMagnitude: readSetting(db, "keywords.minMagnitude"),
          maxKeep: MAX_KEPT_KEYWORDS,
        });
    if (kept.length === 0) {
      throw new NoUsableKeywordsError();
    }
    replaceKeywords(db, stockId, kept, now());
    return {
      ok: true,
      count: kept.length,
      depth,
      dropped,
      candidates: candidates.length,
      rated: rating.rated,
      ...(rating.stopped ? { unratedReason: rating.stopped } : {}),
      headlines: headlines.length,
    };
  } catch (error) {
    // 화면에는 짧은 이유만 보이므로, 자세한 내용은 worker 로그에 남긴다.
    logError(
      `키워드 생성 실패: 종목 id ${stockId}`,
      error instanceof LlmError ? error.message : error,
    );
    const message = describeKeywordError(error);
    setKeywordStatus(db, stockId, "failed", message);
    return { ok: false, error: message };
  }
}

/** Jev 판단 대상(종목·시장·주제) */
export function judgeItem(
  stock: typeof stocks.$inferSelect,
  topicSummary: string | null,
): JudgeItem {
  return {
    kind: stock.kind,
    name: stock.nameKo ?? stock.name,
    officialName:
      stock.nameKo && stock.nameKo !== stock.name ? stock.name : null,
    market: stock.market,
    exchange: stock.exchange,
    code: stock.code,
    summary: topicSummary,
  };
}

/**
 * 후보마다 Jev로 관련도·영향도를 매겨 node에 적는다(11단계). 키가 없거나 예산을 다 썼거나 연달아 실패하면
 * 멈추고 그 이유를 돌려준다(그때는 자르지 않는다). 하나만 실패하면 그 후보는 점수 없이 둔다(잘린다).
 */
async function rateCandidates(
  db: Db,
  decider: Decider | null,
  input: {
    item: JudgeItem;
    nodes: KeywordNode[];
    stockId: number;
    now: () => Date;
  },
): Promise<{ rated: number; stopped?: string }> {
  if (!decider) return { rated: 0, stopped: "TYPESAFE_API_KEY가 없습니다." };
  let rated = 0;
  let failedInRow = 0;
  for (const node of input.nodes) {
    try {
      const rating = await rateWithBudget(
        db,
        decider,
        {
          stock: input.item,
          subject: {
            type: "keyword",
            text: node.textKo,
            path: node.path,
            reason: node.reason || null,
          },
        },
        { purpose: "keyword_rating", now: input.now, stockId: input.stockId },
      );
      node.relevance = rating.relevance;
      node.magnitude = rating.magnitude;
      rated += 1;
      failedInRow = 0;
    } catch (error) {
      if (error instanceof BudgetExceededError) {
        return { rated, stopped: "이번 달 AI 예산을 모두 썼습니다." };
      }
      if (error instanceof DeciderError && error.fatal) {
        return { rated, stopped: error.message };
      }
      failedInRow += 1;
      logError(
        `키워드 평가 실패: ${node.path}`,
        error instanceof Error ? error.message : error,
      );
      if (failedInRow >= MAX_RATING_FAILURES) {
        return {
          rated,
          stopped: `Jev 호출이 ${MAX_RATING_FAILURES}번 연달아 실패했습니다.`,
        };
      }
    }
  }
  return { rated };
}

/**
 * 한 트랜잭션으로 이전 키워드를 지우고 새 트리를 넣는다(읽는 쪽은 이전 또는 새 트리만 본다).
 * 토픽 갱신이 붙인 가지(topic_id가 있는 키워드)는 남긴다(11단계).
 */
export function replaceKeywords(
  db: Db,
  stockId: number,
  tree: KeywordNode[],
  at: Date,
): void {
  db.transaction((tx) => {
    tx.delete(keywords)
      .where(and(eq(keywords.stockId, stockId), isNull(keywords.topicId)))
      .run();
    const ids = new Map<number, number>();
    for (const node of tree) {
      const row = tx
        .insert(keywords)
        .values({
          stockId,
          level: node.level,
          parentId: node.parentKey === null ? null : ids.get(node.parentKey),
          textKo: node.textKo,
          textEn: node.textEn,
          reason: node.reason,
          path: node.path,
          kind: node.kind,
          relevance: node.relevance ?? null,
          magnitude: node.magnitude ?? null,
          createdAt: at,
        })
        .returning({ id: keywords.id })
        .get();
      ids.set(node.key, row.id);
    }
    tx.update(stocks)
      .set({
        keywordsStatus: "ready",
        keywordsError: null,
        keywordsGeneratedAt: at,
      })
      .where(eq(stocks.id, stockId))
      .run();
  });
}

export function setKeywordStatus(
  db: Db,
  stockId: number,
  status: KeywordStatus,
  error: string | null = null,
): void {
  db.update(stocks)
    .set({ keywordsStatus: status, keywordsError: error })
    .where(eq(stocks.id, stockId))
    .run();
}

/** worker가 시작할 때 이어서 만들 종목(대기 중이었거나 만들다 멈춘 것) */
export function stocksAwaitingKeywords(db: Db): number[] {
  return db
    .select({ id: stocks.id })
    .from(stocks)
    .where(
      and(
        isNull(stocks.deletedAt),
        inArray(stocks.keywordsStatus, ["pending", "generating"]),
      ),
    )
    .orderBy(asc(stocks.id))
    .all()
    .map((row) => row.id);
}

function describeKeywordError(error: unknown): string {
  if (error instanceof BudgetExceededError) {
    return "이번 달 AI 예산을 모두 썼습니다.";
  }
  if (error instanceof NoUsableKeywordsError) {
    return "규칙에 맞는 키워드가 나오지 않았습니다.";
  }
  if (error instanceof LlmError) {
    switch (error.kind) {
      case "no_api_key":
        return "OpenRouter API 키가 설정되지 않았습니다.";
      case "payment_required":
        return "OpenRouter 크레딧이 부족하거나 키의 사용 한도에 걸렸습니다.";
      case "rate_limited":
        return "요청이 많아 잠시 막혔습니다.";
      case "timeout":
        return "AI 응답이 늦어 중단했습니다.";
      case "invalid_output":
        return "AI 응답 형식이 올바르지 않았습니다.";
      case "upstream":
        return "AI 서비스에 연결하지 못했거나 오류가 났습니다.";
    }
  }
  return "알 수 없는 오류가 났습니다.";
}
