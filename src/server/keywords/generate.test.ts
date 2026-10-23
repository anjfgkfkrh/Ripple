import { asc, eq } from "drizzle-orm";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import {
  fakeDecider,
  fakeLlm,
  fakeRating,
  insertLink,
  sampleKeywordTree,
  seedListings,
} from "../../../tests/fixtures";
import { DeciderError } from "../ai/decider/types";
import { LlmError, type LlmClient } from "../ai/llm/types";
import { recordUsage } from "../budget/usage";
import { saveFeedHeadlines } from "../collect/feed-headlines";
import { openDb, type Db } from "../db/client";
import { keywords, stocks, topics, usageLogs } from "../db/schema";
import { writeSetting } from "../settings";
import { deleteStock, registerStock } from "../stocks/repository";
import { MAX_KEPT_KEYWORDS } from "./tree";
import {
  addUserTopic,
  attachTopicToStock,
  ensureDefaultMarket,
  syncTopicLenses,
} from "../topics/store";
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
  vi.restoreAllMocks();
});

/** 최근 피드 제목을 넣는다(토픽 갱신 재료, 트리에는 넣지 않음). */
function seedHeadlines(db: Db, at: Date, count = 60) {
  saveFeedHeadlines(
    db,
    "rss:test:economy",
    Array.from({ length: count }, (_, i) => ({
      title: `경제 기사 ${i + 1}`,
      publisher: "테스트",
      publishedAt: new Date(at.getTime() - i * 60_000),
    })),
    at,
  );
}

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

    // Jev가 없으면 후보를 자르지 않고 상한까지 남긴다(11단계).
    expect(result).toEqual({
      ok: true,
      count: 10,
      depth: 2,
      dropped: 0,
      candidates: 10,
      rated: 0,
      unratedReason: "TYPESAFE_API_KEY가 없습니다.",
      headlines: 0,
    });
    expect(llm.calls).toHaveLength(1);
    expect(llm.calls[0]).toMatchObject({
      model: "anthropic/claude-haiku-5.5",
      schemaName: "keyword_tree",
      maxTokens: 16_000,
      timeoutMs: 120_000,
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

  it("그 종목의 최근 주요 기사 제목을 함께 넣고, 넣은 수를 알려 준다", async () => {
    const db = testDb();
    const id = register(db);
    insertLink(db, {
      stockId: id,
      title: "삼성전자 노조 협상",
      publishedAt: NOW,
      relevance: 8,
    });
    const llm = fakeLlm(() => sampleKeywordTree());

    const result = await generateKeywordsForStock(db, llm, id, {
      now: () => LATER,
    });

    expect(result).toMatchObject({ ok: true, headlines: 1 });
    expect(llm.calls[0].messages[1].content).toContain("- 삼성전자 노조 협상");
  });

  it("피드 제목·토픽이 있어도 시장 이슈를 정리하거나 트리에 넣지 않고, 구조 키워드만 만든다(2026-10-23)", async () => {
    const db = testDb();
    const id = register(db);
    seedHeadlines(db, LATER);
    db.insert(topics)
      .values({
        source: "auto",
        name: "중동 긴장",
        terms: ["사우디"],
        createdAt: LATER,
      })
      .run();
    const llm = fakeLlm(() => sampleKeywordTree());
    expect(
      await generateKeywordsForStock(db, llm, id, { now: () => LATER }),
    ).toMatchObject({ ok: true, count: 10 });
    expect(llm.calls.map((call) => call.schemaName)).toEqual(["keyword_tree"]);
    expect(llm.calls[0].messages[1].content).not.toContain("중동 긴장");
    expect(llm.calls[0].messages[1].content).not.toContain("시장 이슈");
    expect(keywordRows(db, id).every((row) => row.kind === "structure")).toBe(
      true,
    );
  });

  it("규칙에 안 맞아 버리거나 단계가 모자라면 LLM 원문을 로그에 남긴다", async () => {
    const db = testDb();
    const id = register(db);
    const logs: string[] = [];
    vi.spyOn(console, "log").mockImplementation((line: string) => {
      logs.push(line);
    });
    // 깊이 2인데 1단계만 준다.
    await generateKeywordsForStock(
      db,
      fakeLlm(() => ({
        keywords: sampleKeywordTree().keywords.filter((k) => k.level === 1),
      })),
      id,
    );
    expect(logs.join("\n")).toContain(
      "키워드 LLM 원문(종목 id 1, 버림 0개, 깊이 1/2)",
    );

    logs.length = 0;
    await generateKeywordsForStock(
      db,
      fakeLlm(() => sampleKeywordTree()),
      id,
    );
    expect(logs.join("\n")).not.toContain("키워드 LLM 원문");
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
    expect(await generateKeywordsForStock(db, llm, id)).toMatchObject({
      ok: true,
      count: 5,
      depth: 1,
      dropped: 5,
      candidates: 5,
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
        {
          level: 1,
          keyword: "반도체",
          keywordEn: "",
          parent: "",
          reason: "r",
        },
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

describe("generateKeywordsForStock: Jev 점수로 고르기 (11단계)", () => {
  it("후보마다 Jev로 관련도·영향도를 매겨 기준 이상만 남기고, 부모가 잘리면 자식도 자른다", async () => {
    const db = testDb();
    const id = register(db);
    // HBM(1단계)만 관련도 미달 → 그 아래 엔비디아·SK하이닉스도 잘린다. 환율은 영향도 미달.
    const decider = fakeDecider(undefined, (input) =>
      input.subject.text === "HBM"
        ? fakeRating({ relevance: 5 })
        : input.subject.text === "환율"
          ? fakeRating({ magnitude: 4.9 })
          : fakeRating(),
    );
    const result = await generateKeywordsForStock(
      db,
      fakeLlm(() => sampleKeywordTree()),
      id,
      { now: () => NOW, decider },
    );
    expect(result).toEqual({
      ok: true,
      count: 6,
      depth: 2,
      dropped: 0,
      candidates: 10,
      rated: 10,
      headlines: 0,
    });
    const rows = keywordRows(db, id);
    expect(rows.map((row) => row.textKo)).toEqual([
      "메모리 반도체",
      "스마트폰",
      "파운드리",
      "D램 가격",
      "TSMC",
      "애플",
    ]);
    expect(rows[0]).toMatchObject({ relevance: 7, magnitude: 6 });
    // Jev 입력: 종목 기준, 키워드 경로와 이유
    expect(decider.rateCalls[0].input).toMatchObject({
      stock: { kind: "stock", name: "삼성전자", code: "005930" },
      subject: {
        type: "keyword",
        text: "메모리 반도체",
        path: "삼성전자 → 메모리 반도체",
        reason: "주력 사업",
      },
    });
    // 평가마다 그 종목의 사용량으로 남긴다(항목별 비용).
    const rating = db
      .select()
      .from(usageLogs)
      .all()
      .filter((row) => row.purpose === "keyword_rating");
    expect(rating).toHaveLength(10);
    expect(rating.every((row) => row.stockId === id)).toBe(true);
  });

  it("기준은 설정 값을 따른다", async () => {
    const db = testDb();
    const id = register(db);
    writeSetting(db, "keywords.minRelevance", 8);
    const result = await generateKeywordsForStock(
      db,
      fakeLlm(() => sampleKeywordTree()),
      id,
      { decider: fakeDecider() },
    );
    // 모두 7점이라 하나도 남지 않으면 실패로 남기고 이전 키워드를 둔다.
    expect(result).toEqual({
      ok: false,
      error: "규칙에 맞는 키워드가 나오지 않았습니다.",
    });
  });

  it("Jev 키가 맞지 않으면 평가를 멈추고 후보를 자르지 않는다", async () => {
    const db = testDb();
    const id = register(db);
    const decider = fakeDecider(
      undefined,
      () => new DeciderError("키가 맞지 않습니다.", "unauthorized"),
    );
    const result = await generateKeywordsForStock(
      db,
      fakeLlm(() => sampleKeywordTree()),
      id,
      { decider },
    );
    expect(result).toMatchObject({
      ok: true,
      count: 10,
      rated: 0,
      unratedReason: "키가 맞지 않습니다.",
    });
    expect(decider.rateCalls).toHaveLength(1);
  });

  it("평가가 3번 연달아 실패하면 멈추고, 안전 상한(40개)까지만 남긴다", async () => {
    const db = testDb();
    const id = register(db);
    const many = {
      keywords: Array.from({ length: 50 }, (_, n) => ({
        level: 1,
        keyword: `후보${n}`,
        keywordEn: "",
        parent: "",
        reason: "r",
      })),
    };
    const decider = fakeDecider(
      undefined,
      () => new DeciderError("일시 오류", "upstream"),
    );
    vi.spyOn(console, "error").mockImplementation(() => {});
    const result = await generateKeywordsForStock(
      db,
      fakeLlm(() => many),
      id,
      { decider },
    );
    expect(result).toMatchObject({
      ok: true,
      count: MAX_KEPT_KEYWORDS,
      candidates: 50,
      rated: 0,
      unratedReason: "Jev 호출이 3번 연달아 실패했습니다.",
    });
    expect(decider.rateCalls).toHaveLength(3);
  });

  it("다시 만들어도 토픽 갱신이 붙인 가지는 남긴다", async () => {
    const db = testDb();
    const id = register(db);
    const topic = db
      .insert(topics)
      .values({
        source: "auto",
        name: "미중 반도체 갈등",
        terms: ["수출 통제"],
        createdAt: NOW,
      })
      .returning()
      .get();
    const stock = db.select().from(stocks).where(eq(stocks.id, id)).get()!;
    attachTopicToStock(
      db,
      stock,
      topic,
      { reason: "수출 규제 영향", relevance: 7, magnitude: 6 },
      NOW,
    );
    await generateKeywordsForStock(
      db,
      fakeLlm(() => sampleKeywordTree()),
      id,
    );
    const attached = keywordRows(db, id).filter((row) => row.topicId !== null);
    expect(attached.map((row) => row.path)).toEqual([
      "삼성전자 → 미중 반도체 갈등",
      "삼성전자 → 미중 반도체 갈등 → 수출 통제",
    ]);
    expect(keywordRows(db, id)).toHaveLength(12);
  });

  it("시장 항목은 시장 기준으로, 종목 기사 제목 없이 만든다", async () => {
    const db = testDb();
    const marketId = ensureDefaultMarket(db, NOW)!;
    const decider = fakeDecider();
    const llm = fakeLlm(() => sampleKeywordTree());
    expect(
      await generateKeywordsForStock(db, llm, marketId, { decider }),
    ).toMatchObject({ ok: true, headlines: 0 });
    expect(llm.calls[0].messages[1].content).toContain(
      "대상: 한국 증시(코스피·코스닥) 전체",
    );
    expect(decider.rateCalls[0].input.stock).toMatchObject({
      kind: "market",
      name: "한국 증시",
    });
  });

  it("사용자 주제는 만들고, 자동 토픽 항목은 만들지 않는다(검색어는 토픽 갱신이 정함)", async () => {
    const db = testDb();
    ensureDefaultMarket(db, NOW);
    const added = addUserTopic(db, "부동산 시장", NOW);
    if (!added.ok) throw new Error("주제를 넣지 못함");
    const llm = fakeLlm(() => sampleKeywordTree());
    expect(
      await generateKeywordsForStock(db, llm, added.lensIds[0]),
    ).toMatchObject({ ok: true });
    expect(llm.calls[0].messages[1].content).toContain("주제: 부동산 시장");

    const auto = db
      .insert(topics)
      .values({
        source: "auto",
        name: "중동 긴장",
        terms: ["사우디"],
        createdAt: NOW,
      })
      .returning()
      .get();
    const [lens] = syncTopicLenses(db, auto, NOW);
    expect(await generateKeywordsForStock(db, llm, lens)).toEqual({
      ok: false,
      error: "자동 토픽은 토픽 갱신이 검색어를 정합니다.",
    });
    expect(llm.calls).toHaveLength(1);
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
