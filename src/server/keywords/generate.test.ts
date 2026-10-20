import { asc, eq } from "drizzle-orm";
import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import {
  fakeLlm,
  sampleKeywordTree,
  seedListings,
} from "../../../tests/fixtures";
import { LlmError, type LlmClient } from "../ai/llm/types";
import { recordUsage } from "../budget/usage";
import { openDb, type Db } from "../db/client";
import { keywords, stocks, usageLogs } from "../db/schema";
import { writeSetting } from "../settings";
import { deleteStock, registerStock } from "../stocks/repository";
import { generateKeywordsForStock, stocksAwaitingKeywords } from "./generate";
import { listKeywordsByStock } from "./read";

const opened: Db[] = [];
const cleanups: (() => void)[] = [];
function testDb(): Db {
  const file = createTestDbFile();
  cleanups.push(file.cleanup);
  const db = openDb(file.path, "readwrite");
  opened.push(db);
  seedListings(db);
  return db;
}
afterEach(() => {
  opened.splice(0).forEach((db) => db.$client.close());
  cleanups.splice(0).forEach((cleanup) => cleanup());
});

const NOW = new Date("2026-10-20T03:00:00Z");
const LATER = new Date("2026-10-21T03:00:00Z");

function register(db: Db, market: "KR" | "US" = "KR"): number {
  const result = registerStock(
    db,
    market === "KR"
      ? { market, code: "005930", userInput: "삼전" }
      : { market, code: "NVDA", userInput: "NVDA", nameKo: "엔비디아" },
  );
  if (result.status === "not_listed") throw new Error("표본에 없는 종목");
  return result.stock.id;
}

function stockRow(db: Db, id: number) {
  return db.select().from(stocks).where(eq(stocks.id, id)).get()!;
}

function keywordRows(db: Db, id: number) {
  return db
    .select()
    .from(keywords)
    .where(eq(keywords.stockId, id))
    .orderBy(asc(keywords.id))
    .all();
}

const failingLlm = (kind: LlmError["kind"]): LlmClient => ({
  provider: "openrouter",
  generateObject: () => Promise.reject(new LlmError("x", kind)),
});

describe("generateKeywordsForStock (SPEC FR-3)", () => {
  it("LLM 1번으로 트리를 만들어 저장하고 상태를 완료로 바꾼다", async () => {
    const db = testDb();
    const id = register(db);
    const llm = fakeLlm(() => sampleKeywordTree());

    const result = await generateKeywordsForStock(db, llm, id, {
      now: () => NOW,
    });

    expect(result).toEqual({ ok: true, count: 10, depth: 2, dropped: 0 });
    expect(llm.calls).toHaveLength(1);
    expect(llm.calls[0]).toMatchObject({
      model: "anthropic/claude-haiku-5.5",
      schemaName: "keyword_tree",
      maxTokens: 4000,
    });
    expect(stockRow(db, id)).toMatchObject({
      keywordsStatus: "ready",
      keywordsError: null,
      keywordsGeneratedAt: NOW,
    });

    const rows = keywordRows(db, id);
    expect(rows).toHaveLength(10);
    const hbm = rows.find((row) => row.textKo === "HBM")!;
    expect(rows.find((row) => row.textKo === "엔비디아")).toMatchObject({
      level: 2,
      parentId: hbm.id,
      textEn: null,
      reason: "HBM 주요 고객",
      path: "삼성전자 → HBM → 엔비디아",
      createdAt: NOW,
    });

    // 호출마다 사용량을 남긴다(비용 장치).
    expect(db.select().from(usageLogs).all()).toEqual([
      expect.objectContaining({
        purpose: "keyword_generation",
        model: "anthropic/claude-haiku-5.5",
      }),
    ]);
  });

  it("만드는 동안에는 상태가 '만드는 중'이다", async () => {
    const db = testDb();
    const id = register(db);
    let statusDuringCall: string | undefined;
    const llm = fakeLlm(() => {
      statusDuringCall = stockRow(db, id).keywordsStatus;
      return sampleKeywordTree();
    });
    await generateKeywordsForStock(db, llm, id);
    expect(statusDuringCall).toBe("generating");
  });

  it("미국 종목은 영어 표기를 함께 저장한다(D-14)", async () => {
    const db = testDb();
    const id = register(db, "US");
    await generateKeywordsForStock(
      db,
      fakeLlm(() => sampleKeywordTree({ english: true })),
      id,
    );
    expect(
      keywordRows(db, id).find((row) => row.textKo === "HBM"),
    ).toMatchObject({ textEn: "HBM", path: "엔비디아 → HBM" });
  });

  it("깊이는 설정 값을 따르고 1~4로 맞춘다", async () => {
    const db = testDb();
    const id = register(db);
    const llm = fakeLlm(() => sampleKeywordTree());

    writeSetting(db, "keywords.depth", 1);
    // 깊이 1이면 2단계 5개는 버린다.
    expect(await generateKeywordsForStock(db, llm, id)).toEqual({
      ok: true,
      count: 5,
      depth: 1,
      dropped: 5,
    });
    expect(llm.calls[0].messages.at(-1)?.content).toContain("깊이: 1");

    writeSetting(db, "keywords.depth", 9);
    await generateKeywordsForStock(db, llm, id);
    expect(llm.calls[1].messages.at(-1)?.content).toContain("깊이: 4");
  });

  it("다시 만들면 이전 키워드를 통째로 바꾼다(D-15·D-16)", async () => {
    const db = testDb();
    const id = register(db);
    await generateKeywordsForStock(
      db,
      fakeLlm(() => sampleKeywordTree()),
      id,
      { now: () => NOW },
    );
    const replacement = {
      keywords: [
        { level: 1, keyword: "반도체", keywordEn: "", parent: "", reason: "r" },
      ],
    };
    await generateKeywordsForStock(
      db,
      fakeLlm(() => replacement),
      id,
      { now: () => LATER },
    );

    expect(keywordRows(db, id).map((row) => row.path)).toEqual([
      "삼성전자 → 반도체",
    ]);
    expect(stockRow(db, id).keywordsGeneratedAt).toEqual(LATER);
  });

  it("실패하면 이유를 남기고 이전 키워드는 그대로 둔다", async () => {
    const db = testDb();
    const id = register(db);
    await generateKeywordsForStock(
      db,
      fakeLlm(() => sampleKeywordTree()),
      id,
      { now: () => NOW },
    );

    const result = await generateKeywordsForStock(
      db,
      failingLlm("upstream"),
      id,
      { now: () => LATER },
    );

    expect(result).toEqual({
      ok: false,
      error: "AI 서비스에 연결하지 못했거나 오류가 났습니다.",
    });
    expect(stockRow(db, id)).toMatchObject({
      keywordsStatus: "failed",
      keywordsError: "AI 서비스에 연결하지 못했거나 오류가 났습니다.",
      keywordsGeneratedAt: NOW,
    });
    expect(keywordRows(db, id)).toHaveLength(10);
  });

  it.each([
    ["no_api_key", "OpenRouter API 키가 설정되지 않았습니다."],
    [
      "payment_required",
      "OpenRouter 크레딧이 부족하거나 키의 사용 한도에 걸렸습니다.",
    ],
    ["rate_limited", "요청이 많아 잠시 막혔습니다."],
    ["timeout", "AI 응답이 늦어 중단했습니다."],
    ["invalid_output", "AI 응답 형식이 올바르지 않았습니다."],
  ] as const)("LLM 오류 %s → 화면에 보일 이유", async (kind, message) => {
    const db = testDb();
    const id = register(db);
    expect(await generateKeywordsForStock(db, failingLlm(kind), id)).toEqual({
      ok: false,
      error: message,
    });
    expect(stockRow(db, id).keywordsError).toBe(message);
  });

  it("LLM 오류가 아닌 예기치 못한 오류도 실패로 남긴다", async () => {
    const db = testDb();
    const id = register(db);
    const llm: LlmClient = {
      provider: "openrouter",
      generateObject: () => Promise.reject(new Error("boom")),
    };
    expect(await generateKeywordsForStock(db, llm, id)).toEqual({
      ok: false,
      error: "알 수 없는 오류가 났습니다.",
    });
    expect(stockRow(db, id).keywordsStatus).toBe("failed");
  });

  it("예산을 다 썼으면 LLM을 부르지 않고 실패로 남긴다", async () => {
    const db = testDb();
    const id = register(db);
    recordUsage(db, {
      provider: "openrouter",
      model: "m",
      purpose: "t",
      usage: { inputTokens: 1, outputTokens: 1, costUsd: 5 },
    });
    const llm = fakeLlm(() => sampleKeywordTree());

    expect(await generateKeywordsForStock(db, llm, id)).toEqual({
      ok: false,
      error: "이번 달 AI 예산을 모두 썼습니다.",
    });
    expect(llm.calls).toHaveLength(0);
    expect(stockRow(db, id).keywordsStatus).toBe("failed");
  });

  it("규칙에 맞는 키워드가 하나도 없으면 실패", async () => {
    const db = testDb();
    const id = register(db);
    const llm = fakeLlm(() => ({
      keywords: [
        {
          level: 1,
          keyword: "삼성전자",
          keywordEn: "",
          parent: "",
          reason: "",
        },
      ],
    }));
    expect(await generateKeywordsForStock(db, llm, id)).toEqual({
      ok: false,
      error: "규칙에 맞는 키워드가 나오지 않았습니다.",
    });
    expect(keywordRows(db, id)).toEqual([]);
  });

  it("없거나 삭제한 종목은 LLM을 부르지 않는다", async () => {
    const db = testDb();
    const id = register(db);
    deleteStock(db, id);
    const llm = fakeLlm(() => sampleKeywordTree());

    expect(await generateKeywordsForStock(db, llm, id)).toMatchObject({
      ok: false,
    });
    expect(await generateKeywordsForStock(db, llm, id + 100)).toMatchObject({
      ok: false,
    });
    expect(llm.calls).toHaveLength(0);
  });
});

describe("stocksAwaitingKeywords", () => {
  it("삭제하지 않은 종목 중 대기·만드는 중인 것만 id 순서로", async () => {
    const db = testDb();
    const samsung = register(db);
    const nvidia = register(db, "US");
    expect(stocksAwaitingKeywords(db)).toEqual([samsung, nvidia]);

    await generateKeywordsForStock(
      db,
      fakeLlm(() => sampleKeywordTree()),
      samsung,
    );
    expect(stocksAwaitingKeywords(db)).toEqual([nvidia]);

    await generateKeywordsForStock(db, failingLlm("upstream"), nvidia);
    expect(stocksAwaitingKeywords(db)).toEqual([]);

    db.update(stocks).set({ keywordsStatus: "generating" }).run();
    deleteStock(db, nvidia);
    expect(stocksAwaitingKeywords(db)).toEqual([samsung]);
  });
});

describe("listKeywordsByStock", () => {
  it("종목별로 묶고 단계·생성 순서대로, 키워드가 없는 종목은 빈 목록", async () => {
    const db = testDb();
    const samsung = register(db);
    const nvidia = register(db, "US");
    await generateKeywordsForStock(
      db,
      fakeLlm(() => ({
        keywords: [...sampleKeywordTree().keywords].reverse(),
      })),
      samsung,
    );

    const grouped = listKeywordsByStock(db, [samsung, nvidia]);
    const levels = grouped.get(samsung)!.map((row) => row.level);
    expect(levels).toEqual([1, 1, 1, 1, 1, 2, 2, 2, 2, 2]);
    expect(grouped.get(nvidia)).toEqual([]);
    expect(listKeywordsByStock(db, []).size).toBe(0);
  });
});
