import { asc, eq } from "drizzle-orm";
import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import {
  fakeDecider,
  fakeDecision,
  seedListings,
} from "../../../tests/fixtures";
import { DeciderError } from "../ai/decider/types";
import { recordUsage } from "../budget/usage";
import { openDb, type Db } from "../db/client";
import {
  articleMatches,
  articles,
  evaluations,
  stocks,
  topics,
  usageLogs,
  type MatchKind,
} from "../db/schema";
import { writeSetting } from "../settings";
import { deleteStock, registerStock } from "../stocks/repository";
import { ensureDefaultMarket, syncTopicLenses } from "../topics/store";
import { countPendingEvaluations, evaluationTotals } from "./read";
import { evaluatePending, MAX_CONSECUTIVE_FAILURES } from "./run";

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

// 한국 시간 2026-10-20 12:00
const NOW = new Date("2026-10-20T03:00:00Z");
const HOUR = 60 * 60 * 1000;
const ago = (hours: number) => new Date(NOW.getTime() - hours * HOUR);

function addStocks(db: Db) {
  const samsung = registerStock(db, {
    market: "KR",
    code: "005930",
    userInput: "삼전",
  });
  const nvidia = registerStock(db, {
    market: "US",
    code: "NVDA",
    userInput: "NVDA",
    nameKo: "엔비디아",
  });
  if (samsung.status === "not_listed" || nvidia.status === "not_listed")
    throw new Error("표본에 없음");
  return { samsung: samsung.stock.id, nvidia: nvidia.stock.id };
}

/**
 * 서로 다른 소식의 제목. 같은 낱말이 많이 겹치면(예: '삼성전자 소식 1'·'삼성전자 소식 2') 같은 소식으로 보고
 * 앞선 평가를 그대로 쓰므로(11단계) 평가 수를 세는 시험에는 이것을 쓴다.
 */
const DISTINCT = [
  "반도체 수출 급증",
  "노조 파업 협상",
  "스마트폰 신제품 공개",
  "파운드리 대형 수주",
  "배당 확대 발표",
  "환율 하락 영향",
  "인도 공장 착공",
  "특허 소송 승소",
];

let serial = 0;
/** 기사 하나와 종목 연결 하나를 넣는다. */
function addLink(
  db: Db,
  stockId: number,
  title: string,
  publishedAt: Date,
  link: { kind?: MatchKind; path?: string; reason?: string | null } = {},
): number {
  serial += 1;
  const article = db
    .insert(articles)
    .values({
      urlKey: `n.example/${serial}`,
      url: `https://n.example/${serial}`,
      title,
      summary: `${title} 요약`,
      publisher: "가경제",
      feed: "rss:a:economy",
      publishedAt,
      collectedAt: NOW,
    })
    .returning({ id: articles.id })
    .get();
  const kind = link.kind ?? "name";
  return db
    .insert(articleMatches)
    .values({
      articleId: article.id,
      stockId,
      keywordPath: link.path ?? "삼성전자",
      keywordReason: link.reason ?? null,
      matchKind: kind,
      level: kind === "keyword" ? 1 : 0,
      createdAt: NOW,
    })
    .returning({ id: articleMatches.id })
    .get().id;
}

const evaluate = (db: Db, decider: ReturnType<typeof fakeDecider> | null) =>
  evaluatePending({ db, decider, now: () => NOW });

describe("evaluatePending (SPEC FR-5)", () => {
  it("평가하지 않은 연결을 종목 이름·직접 연결 먼저, 그다음 최근 기사부터 평가해 저장하고, 쓴 비용을 기록한다", async () => {
    const db = testDb();
    const ids = addStocks(db);
    const oldest = addLink(db, ids.samsung, "삼성전자 실적", ago(3));
    const newest = addLink(db, ids.samsung, "HBM 증설", ago(1), {
      kind: "keyword",
      path: "삼성전자 → HBM 메모리",
      reason: "주력 메모리 제품",
    });
    const middle = addLink(db, ids.nvidia, "엔비디아 신제품", ago(2), {
      path: "엔비디아",
    });
    const scores: Record<string, number> = {
      "삼성전자 실적": 8,
      "HBM 증설": 4,
      "엔비디아 신제품": 6,
    };
    const decider = fakeDecider((input) =>
      fakeDecision({ relevance: scores[input.article.title] }),
    );

    const summary = await evaluate(db, decider);

    expect(summary).toMatchObject({
      evaluated: 3,
      // 기본 표시 기준 6점 이상(6점 포함)
      relevant: 2,
      failed: 0,
      remaining: 0,
    });
    expect(summary.note).toBeUndefined();
    // 입력 1000토큰 × 100만 토큰당 $0.042 × 3회
    expect(summary.costUsd).toBeCloseTo(0.000126, 9);

    // D-05: 이름 연결(최근 것부터) 다음에 키워드 연결
    expect(decider.calls.map((c) => c.input.article.title)).toEqual([
      "엔비디아 신제품",
      "삼성전자 실적",
      "HBM 증설",
    ]);
    expect(decider.calls.every((c) => c.model === "jev-1.13.0")).toBe(true);
    const nvidiaName = db
      .select({ name: stocks.name })
      .from(stocks)
      .where(eq(stocks.id, ids.nvidia))
      .get()!.name;
    expect(decider.calls[0].input).toEqual({
      stock: {
        kind: "stock",
        name: "엔비디아",
        officialName: nvidiaName,
        market: "US",
        exchange: expect.any(String),
        code: "NVDA",
        summary: null,
      },
      link: { kind: "name", path: "엔비디아", reason: null },
      article: { title: "엔비디아 신제품", summary: "엔비디아 신제품 요약" },
    });
    expect(decider.calls[2].input.stock.officialName).toBeNull();
    expect(decider.calls[2].input.link).toEqual({
      kind: "keyword",
      path: "삼성전자 → HBM 메모리",
      reason: "주력 메모리 제품",
    });

    const saved = db
      .select()
      .from(evaluations)
      .orderBy(asc(evaluations.matchId))
      .all();
    expect(
      saved.map((e) => [e.matchId, e.relevance, e.impact, e.model]),
    ).toEqual([
      [oldest, 8, "positive", "jev-1.13.0"],
      [newest, 4, "positive", "jev-1.13.0"],
      [middle, 6, "positive", "jev-1.13.0"],
    ]);
    expect(saved[0]).toMatchObject({
      stockId: ids.samsung,
      relevanceConfidence: 0.7,
      impactConfidence: 0.8,
      impactProbabilities: { positive: 0.8, negative: 0.1, neutral: 0.1 },
      evaluatedAt: NOW,
    });

    const usage = db.select().from(usageLogs).all();
    expect(usage).toHaveLength(3);
    expect(usage[0]).toMatchObject({
      provider: "typesafe",
      model: "jev-1.13.0",
      purpose: "article_evaluation",
      inputTokens: 1000,
      outputTokens: 50,
      at: NOW,
    });
    expect(usage[0].costUsd).toBeCloseTo(0.000042, 9);
  });

  it("회차 한도(D-03)까지만 평가하고 나머지는 다음 회차로, 7일 지난 기사는 평가하지 않는다(D-05)", async () => {
    const db = testDb();
    const ids = addStocks(db);
    for (let i = 0; i < 5; i += 1) {
      addLink(db, ids.samsung, DISTINCT[i], ago(i + 1));
    }
    addLink(db, ids.samsung, "삼성전자 8일 전 소식", ago(8 * 24));
    writeSetting(db, "evaluation.maxPerRun", 3);
    const decider = fakeDecider();

    expect(await evaluate(db, decider)).toMatchObject({
      evaluated: 3,
      remaining: 2,
      note: "회차 한도 3건까지 평가했습니다. 남은 2건은 다음 회차에 평가합니다.",
    });
    expect(await evaluate(db, decider)).toMatchObject({
      evaluated: 2,
      remaining: 0,
    });
    expect(decider.calls.map((c) => c.input.article.title)).not.toContain(
      "삼성전자 8일 전 소식",
    );
    expect(countPendingEvaluations(db, NOW)).toBe(0);
  });

  it("이미 평가한 연결과 삭제한 종목의 연결은 다시 평가하지 않는다", async () => {
    const db = testDb();
    const ids = addStocks(db);
    addLink(db, ids.samsung, "삼성전자 실적", ago(1));
    const decider = fakeDecider();
    await evaluate(db, decider);

    addLink(db, ids.nvidia, "엔비디아 신제품", ago(1), { path: "엔비디아" });
    deleteStock(db, ids.nvidia);
    expect(await evaluate(db, decider)).toMatchObject({
      evaluated: 0,
      remaining: 0,
    });
    expect(decider.calls).toHaveLength(1);
  });

  it("설정한 모델 버전과 표시 기준을 쓴다", async () => {
    const db = testDb();
    const ids = addStocks(db);
    addLink(db, ids.samsung, "삼성전자 실적", ago(1));
    writeSetting(db, "jev.model", "jev-1.14.0");
    writeSetting(db, "evaluation.minRelevance", 8);
    const decider = fakeDecider(() =>
      fakeDecision({ relevance: 7.9, model: "jev-1.14.0" }),
    );

    expect(await evaluate(db, decider)).toMatchObject({
      evaluated: 1,
      relevant: 0,
    });
    expect(decider.calls[0].model).toBe("jev-1.14.0");
    expect(db.select().from(evaluations).get()?.model).toBe("jev-1.14.0");
  });

  it("키가 없으면(decider=null) 평가하지 않고 남은 수와 이유를 돌려준다", async () => {
    const db = testDb();
    const ids = addStocks(db);
    addLink(db, ids.samsung, "삼성전자 실적", ago(1));
    addLink(db, ids.samsung, "삼성전자 투자", ago(2));

    expect(await evaluate(db, null)).toMatchObject({
      evaluated: 0,
      remaining: 2,
      note: "TYPESAFE_API_KEY가 없어 평가하지 않았습니다.",
    });
    expect(db.select().from(evaluations).all()).toEqual([]);
  });

  it("이번 달 예산을 이미 다 썼으면 부르지 않는다", async () => {
    const db = testDb();
    const ids = addStocks(db);
    addLink(db, ids.samsung, "삼성전자 실적", ago(1));
    writeSetting(db, "budget.monthlyUsd", 0.01);
    recordUsage(db, {
      provider: "openrouter",
      model: "m",
      purpose: "keyword_generation",
      usage: { inputTokens: 1, outputTokens: 1, costUsd: 0.01 },
      at: NOW,
    });
    const decider = fakeDecider();

    expect(await evaluate(db, decider)).toMatchObject({
      evaluated: 0,
      remaining: 1,
      note: "이번 달 AI 예산을 모두 써서 평가를 멈췄습니다.",
    });
    expect(decider.calls).toHaveLength(0);
  });

  it("평가 도중 예산을 넘기면 멈추고 나머지는 다음으로 남긴다", async () => {
    const db = testDb();
    const ids = addStocks(db);
    for (let i = 0; i < 8; i += 1) {
      addLink(db, ids.samsung, DISTINCT[i], ago(i + 1));
    }
    // 한 번에 $0.000042. 두 번째 호출 전에는 한도 아래, 세 번째 호출 전에는 한도 이상.
    writeSetting(db, "budget.monthlyUsd", 0.00005);
    const decider = fakeDecider();

    expect(await evaluate(db, decider)).toMatchObject({
      evaluated: 2,
      remaining: 6,
      note: "이번 달 AI 예산을 모두 써서 평가를 멈췄습니다.",
    });
    expect(decider.calls).toHaveLength(2);
  });

  it("그 밖의 오류는 실패로 세고 계속하며, 다음 회차에 다시 평가한다", async () => {
    const db = testDb();
    const ids = addStocks(db);
    addLink(db, ids.samsung, DISTINCT[0], ago(1));
    addLink(db, ids.samsung, DISTINCT[1], ago(2));
    let down = true;
    const decider = fakeDecider((input) =>
      down && input.article.title === DISTINCT[1]
        ? new DeciderError("TypeSafe 오류(HTTP 503)", "upstream")
        : fakeDecision(),
    );

    expect(await evaluate(db, decider)).toMatchObject({
      evaluated: 1,
      failed: 1,
      remaining: 0,
    });
    down = false;
    expect(await evaluate(db, decider)).toMatchObject({
      evaluated: 1,
      failed: 0,
    });
    expect(db.select().from(evaluations).all()).toHaveLength(2);
  });

  it("키가 맞지 않으면 이번 회차 평가를 멈추고 모두 남긴다", async () => {
    const db = testDb();
    const ids = addStocks(db);
    for (let i = 0; i < 6; i += 1) {
      addLink(db, ids.samsung, DISTINCT[i], ago(i + 1));
    }
    const decider = fakeDecider(
      () =>
        new DeciderError(
          "TypeSafe 키가 맞지 않거나 권한이 없습니다.",
          "unauthorized",
        ),
    );

    expect(await evaluate(db, decider)).toMatchObject({
      evaluated: 0,
      failed: 0,
      remaining: 6,
      note: "TypeSafe 키가 맞지 않거나 권한이 없습니다.",
    });
    expect(decider.calls).toHaveLength(1);
  });

  it(`${MAX_CONSECUTIVE_FAILURES}번 연달아 실패하면(장애) 멈추고 나머지는 남긴다`, async () => {
    const db = testDb();
    const ids = addStocks(db);
    for (let i = 0; i < 6; i += 1) {
      addLink(db, ids.samsung, DISTINCT[i], ago(i + 1));
    }
    const decider = fakeDecider(
      () => new DeciderError("TypeSafe에 연결하지 못했습니다.", "upstream"),
    );

    const summary = await evaluate(db, decider);

    expect(summary).toMatchObject({
      evaluated: 0,
      failed: MAX_CONSECUTIVE_FAILURES,
      remaining: 6 - MAX_CONSECUTIVE_FAILURES,
    });
    expect(summary.note).toContain("연달아 실패");
    expect(decider.calls).toHaveLength(MAX_CONSECUTIVE_FAILURES);
  });
});

describe("evaluationTotals", () => {
  it("평가 수와 표시 기준(설정 값) 이상인 수", async () => {
    const db = testDb();
    const ids = addStocks(db);
    addLink(db, ids.samsung, "가", ago(1));
    addLink(db, ids.samsung, "나", ago(2));
    addLink(db, ids.samsung, "다", ago(3));
    const scores: Record<string, number> = { 가: 6, 나: 5.9, 다: 9 };
    await evaluate(
      db,
      fakeDecider((input) =>
        fakeDecision({ relevance: scores[input.article.title] }),
      ),
    );

    expect(evaluationTotals(db)).toEqual({
      evaluated: 3,
      relevant: 2,
      minRelevance: 6,
      minMagnitude: 6,
    });
    writeSetting(db, "evaluation.minRelevance", 8);
    expect(evaluationTotals(db).relevant).toBe(1);
    // 영향도 기준도 함께 본다(기본 결과는 영향도 7점).
    writeSetting(db, "evaluation.minRelevance", 6);
    writeSetting(db, "evaluation.minMagnitude", 7.5);
    expect(evaluationTotals(db).relevant).toBe(0);
  });
});

describe("evaluatePending: 11단계", () => {
  it("영향도도 저장하고, 표시 기준은 관련도·영향도가 둘 다 기준 이상일 때다", async () => {
    const db = testDb();
    const ids = addStocks(db);
    addLink(db, ids.samsung, DISTINCT[0], ago(1));
    addLink(db, ids.samsung, DISTINCT[1], ago(2));
    addLink(db, ids.samsung, DISTINCT[2], ago(3));
    const scores: Record<string, [number, number]> = {
      [DISTINCT[0]]: [8, 5.9], // 영향도 미달
      [DISTINCT[1]]: [6, 6], // 경계값은 보인다
      [DISTINCT[2]]: [5.9, 9], // 관련도 미달
    };
    const decider = fakeDecider((input) => {
      const [relevance, magnitude] = scores[input.article.title];
      return fakeDecision({ relevance, magnitude, magnitudeConfidence: 0.6 });
    });

    expect(await evaluate(db, decider)).toMatchObject({
      evaluated: 3,
      relevant: 1,
      copied: 0,
    });
    expect(
      db
        .select({
          magnitude: evaluations.magnitude,
          confidence: evaluations.magnitudeConfidence,
        })
        .from(evaluations)
        .orderBy(asc(evaluations.id))
        .all(),
    ).toEqual([
      { magnitude: 5.9, confidence: 0.6 },
      { magnitude: 6, confidence: 0.6 },
      { magnitude: 9, confidence: 0.6 },
    ]);
    // 사용량은 그 항목의 것으로 남긴다(항목별 비용).
    expect(
      db
        .select({ stockId: usageLogs.stockId })
        .from(usageLogs)
        .all()
        .every((row) => row.stockId === ids.samsung),
    ).toBe(true);
  });

  it("같은 항목에 24시간 안에 걸린 같은 소식은 앞선 평가를 그대로 쓰고 Jev를 부르지 않는다", async () => {
    const db = testDb();
    const ids = addStocks(db);
    const title = "한은 기준금리 동결…연 2.5% 유지";
    const copy = addLink(db, ids.samsung, title, ago(5));
    addLink(db, ids.samsung, "[속보] 한은, 기준금리 동결…연 2.5% 유지", ago(4));
    // 다른 항목이면 따로 평가한다.
    addLink(db, ids.nvidia, title, ago(4), { path: "엔비디아" });
    // 24시간 넘게 떨어지면 따로 평가한다.
    addLink(db, ids.samsung, title, ago(40));
    // 같은 소식 복사는 회차 한도에 세지 않는다.
    writeSetting(db, "evaluation.maxPerRun", 3);
    const decider = fakeDecider(() => fakeDecision({ relevance: 8 }));

    const summary = await evaluate(db, decider);

    expect(summary).toMatchObject({
      evaluated: 3,
      copied: 1,
      relevant: 4,
      remaining: 0,
    });
    expect(decider.calls).toHaveLength(3);
    const copied = db
      .select()
      .from(evaluations)
      .where(eq(evaluations.matchId, copy))
      .get()!;
    const original = db
      .select()
      .from(evaluations)
      .where(eq(evaluations.id, copied.copiedFrom!))
      .get()!;
    expect(copied).toMatchObject({
      relevance: original.relevance,
      magnitude: original.magnitude,
      impact: original.impact,
      model: original.model,
    });
    expect(original.copiedFrom).toBeNull();
    // 복사는 비용을 쓰지 않는다.
    expect(db.select().from(usageLogs).all()).toHaveLength(3);
  });

  it("시장·주제 항목은 그 종류와 주제 설명을 Jev 입력에 넣는다", async () => {
    const db = testDb();
    const market = ensureDefaultMarket(db, NOW)!;
    const topic = db
      .insert(topics)
      .values({
        source: "auto",
        name: "중동 긴장",
        summary: "사우디 시설 공격 위협",
        terms: ["사우디"],
        createdAt: NOW,
      })
      .returning()
      .get();
    const [lens] = syncTopicLenses(db, topic, NOW);
    addLink(db, market, DISTINCT[0], ago(1), {
      kind: "keyword",
      path: "한국 증시 → 수출",
    });
    addLink(db, lens, DISTINCT[1], ago(2), {
      kind: "keyword",
      path: "중동 긴장 → 사우디",
    });
    const decider = fakeDecider();

    await evaluate(db, decider);

    expect(decider.calls.map((c) => c.input.stock)).toEqual([
      expect.objectContaining({
        kind: "market",
        name: "한국 증시",
        market: "KR",
      }),
      expect.objectContaining({
        kind: "topic",
        name: "중동 긴장",
        summary: "사우디 시설 공격 위협",
      }),
    ]);
  });
});
