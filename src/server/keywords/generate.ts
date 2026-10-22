import { and, asc, eq, inArray, isNull } from "drizzle-orm";
import { LlmError, type LlmClient } from "@/server/ai/llm/types";
import { BudgetExceededError, generateWithBudget } from "@/server/budget/usage";
import type { Db } from "@/server/db/client";
import { keywords, stocks, type KeywordStatus } from "@/server/db/schema";
import { log, logError } from "@/server/log";
import { readSetting } from "@/server/settings";
import { recentHeadlines } from "./headlines";
import { ensureMarketIssues } from "./market-issues";
import {
  buildKeywordMessages,
  buildKeywordTree,
  clampDepth,
  keywordTreeSchema,
  type KeywordNode,
} from "./tree";

/** LLM은 답했지만 규칙에 맞는 키워드가 하나도 없을 때 */
class NoUsableKeywordsError extends Error {}

/** 2026-10-22 실측: 깊이 3에 14~16초. 깊이 4와 느린 때를 생각해 넉넉히 둔다. */
const KEYWORD_TIMEOUT_MS = 60_000;
/** 규칙에 안 맞는 답을 볼 때 로그에 남기는 LLM 원문 길이 */
const RAW_LOG_MAX_CHARS = 6000;

/** 실패 이유(error)는 화면에 "키워드를 만들지 못했습니다." 뒤에 붙여 보여 준다. */
export type GenerateResult =
  | {
      ok: true;
      count: number;
      depth: number;
      /** LLM이 줬지만 규칙(단계별 5개 등)에 맞지 않아 버린 키워드 수 */
      dropped: number;
      /** 참고로 넣은 최근 기사 제목 수 */
      headlines: number;
      /** 참고로 넣은 오늘의 시장 이슈 수(10단계) */
      issues: number;
    }
  | { ok: false; error: string };

/**
 * 종목 하나의 키워드 트리를 LLM 1번 호출로 만든다(SPEC FR-3).
 * 성공하면 이전 키워드를 통째로 바꾸고(D-16), 실패하면 이전 키워드를 그대로 두고 이유를 남긴다.
 */
export async function generateKeywordsForStock(
  db: Db,
  llm: LlmClient,
  stockId: number,
  options: { now?: () => Date } = {},
): Promise<GenerateResult> {
  const now = options.now ?? (() => new Date());
  const stock = db.select().from(stocks).where(eq(stocks.id, stockId)).get();
  if (!stock || stock.deletedAt) {
    return { ok: false, error: "종목이 없거나 삭제됐습니다." };
  }

  setKeywordStatus(db, stockId, "generating");
  const depth = clampDepth(readSetting(db, "keywords.depth"));
  const headlines = recentHeadlines(db, stockId, now());
  // 하루 한 번 정리하는 시장 이슈(모든 종목 공용). 실패해도 던지지 않고 빈 목록이다.
  const issues = await ensureMarketIssues(db, llm, now);
  try {
    const { value } = await generateWithBudget(
      db,
      llm,
      {
        model: readSetting(db, "llm.model"),
        messages: buildKeywordMessages(stock, depth, { headlines, issues }),
        schema: keywordTreeSchema,
        schemaName: "keyword_tree",
        // 추론 토큰과 함께 쓴다. 깊이 3에 실측 약 3,000개(2026-10-22)라 깊이 4에도 넉넉하게.
        maxTokens: 8000,
        timeoutMs: KEYWORD_TIMEOUT_MS,
      },
      { purpose: "keyword_generation", now },
    );
    const tree = buildKeywordTree(stock, value.keywords, depth);
    const dropped = value.keywords.length - tree.length;
    const deepest = Math.max(0, ...tree.map((node) => node.level));
    if (dropped > 0 || deepest < depth) {
      // 규칙에 맞지 않는 답(단계 누락 등)의 원인을 볼 수 있게 원문을 남긴다(10단계 결정).
      log(
        `키워드 LLM 원문(종목 id ${stockId}, 버림 ${dropped}개, 깊이 ${deepest}/${depth}): ${JSON.stringify(value.keywords).slice(0, RAW_LOG_MAX_CHARS)}`,
      );
    }
    if (tree.length === 0) {
      throw new NoUsableKeywordsError();
    }
    replaceKeywords(db, stockId, tree, now());
    return {
      ok: true,
      count: tree.length,
      depth,
      dropped,
      headlines: headlines.length,
      issues: issues.length,
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

/** 한 트랜잭션으로 이전 키워드를 지우고 새 트리를 넣는다(읽는 쪽은 이전 또는 새 트리만 본다). */
export function replaceKeywords(
  db: Db,
  stockId: number,
  tree: KeywordNode[],
  at: Date,
): void {
  db.transaction((tx) => {
    tx.delete(keywords).where(eq(keywords.stockId, stockId)).run();
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
