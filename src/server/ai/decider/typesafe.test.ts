import { describe, expect, it } from "vitest";
import {
  buildArticleQuestions,
  buildArticleState,
  buildLeadQuestions,
  buildLeadState,
} from "./questions";
import { createTypeSafeDecider } from "./typesafe";
import { DeciderError, type DecisionInput, type LeadInput } from "./types";

const INPUT: DecisionInput = {
  stock: {
    kind: "stock",
    name: "삼성전자",
    officialName: null,
    market: "KR",
    exchange: "KOSPI",
    code: "005930",
  },
  link: { kind: "name", path: "삼성전자", reason: null },
  article: { title: "삼성전자 3분기 실적", summary: "영업이익이 늘었다." },
};

const LEAD: LeadInput = {
  stock: INPUT.stock,
  subject: {
    type: "keyword",
    text: "HBM",
    path: "삼성전자 → HBM",
    reason: "AI 서버용 고대역폭 메모리",
  },
};

const scoreAnswer = (score: number, confidence: number) => ({
  type: "score",
  score,
  confidence,
  legend: {},
  probabilities: { 0: 0, 1: 0, 2: 0.1, 3: 0.3, 4: 0.5, 5: 0.1 },
});

/** 공식 문서(API reference)의 응답 모양 */
const answer = (overrides: Record<string, unknown> = {}) => ({
  model: "jev-1.13.0",
  answers: {
    relevance: scoreAnswer(3.6, 0.7),
    magnitude: scoreAnswer(2.5, 0.6),
    impact: {
      type: "choice",
      choice: "positive",
      confidence: 0.8,
      probabilities: { positive: 0.85, negative: 0.05, neutral: 0.1 },
    },
  },
  usage: { input_tokens: 900, output_tokens: 50 },
  ...overrides,
});

type Call = { url: string; headers: Headers; body: unknown };

/** 받은 요청을 남기고 차례대로 응답하는 가짜 fetch */
function fakeFetch(...responses: (() => Response)[]) {
  const calls: Call[] = [];
  const fetchImpl = (async (
    url: string | URL | Request,
    init?: RequestInit,
  ) => {
    calls.push({
      url: String(url),
      headers: new Headers(init?.headers),
      body: JSON.parse(String(init?.body)),
    });
    const next = responses[Math.min(calls.length, responses.length) - 1];
    return next();
  }) as typeof fetch;
  return { fetchImpl, calls };
}

const json = (status: number, body: unknown, headers: HeadersInit = {}) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", ...headers },
  });

const decider = (fetchImpl: typeof fetch, apiKey = "ts-test") =>
  createTypeSafeDecider({
    apiKey,
    baseUrl: "http://typesafe.test",
    fetchImpl,
  });

describe("createTypeSafeDecider", () => {
  it("한 요청에 세 질문(관련도·영향도·호재/악재)을 보내고, 점수는 ×2 해서 0~10으로 돌려준다", async () => {
    const { fetchImpl, calls } = fakeFetch(() => json(200, answer()));

    const decision = await decider(fetchImpl).decide(INPUT, "jev-1.13.0");

    expect(decision).toEqual({
      relevance: 7.2,
      relevanceConfidence: 0.7,
      magnitude: 5,
      magnitudeConfidence: 0.6,
      impact: "positive",
      impactConfidence: 0.8,
      impactProbabilities: { positive: 0.85, negative: 0.05, neutral: 0.1 },
      model: "jev-1.13.0",
      usage: { inputTokens: 900, outputTokens: 50 },
    });
    expect(calls).toHaveLength(1);
    expect(calls[0].url).toBe("http://typesafe.test/v1/systemone");
    expect(calls[0].headers.get("authorization")).toBe("Bearer ts-test");
    // 압축하지 않고 보낸다(요청이 작음).
    expect(calls[0].headers.get("content-encoding")).toBeNull();
    expect(calls[0].body).toEqual({
      model: "jev-1.13.0",
      state: buildArticleState(INPUT),
      questions: JSON.parse(JSON.stringify(buildArticleQuestions("stock"))),
    });
  });

  it("키워드·토픽 평가(rate)는 관련도·영향도 두 질문을 보낸다(11단계)", async () => {
    const { fetchImpl, calls } = fakeFetch(() =>
      json(200, {
        model: "jev-1.13.0",
        answers: {
          relevance: scoreAnswer(4, 0.8),
          magnitude: scoreAnswer(3.1, 0.5),
        },
        usage: { input_tokens: 400, output_tokens: 20 },
      }),
    );
    const rating = await decider(fetchImpl).rate(LEAD, "jev-1.13.0");
    expect(rating).toEqual({
      relevance: 8,
      relevanceConfidence: 0.8,
      magnitude: 6.2,
      magnitudeConfidence: 0.5,
      model: "jev-1.13.0",
      usage: { inputTokens: 400, outputTokens: 20 },
    });
    expect(calls[0].body).toEqual({
      model: "jev-1.13.0",
      state: buildLeadState(LEAD),
      questions: JSON.parse(JSON.stringify(buildLeadQuestions("stock"))),
    });
  });

  it("시장 항목은 시장 질문으로 묻는다", async () => {
    const { fetchImpl, calls } = fakeFetch(() => json(200, answer()));
    await decider(fetchImpl).decide(
      {
        ...INPUT,
        stock: {
          kind: "market",
          name: "한국 증시",
          officialName: null,
          market: "KR",
          exchange: "KOSPI·KOSDAQ",
          code: "MARKET",
        },
      },
      "jev-1.13.0",
    );
    expect((calls[0].body as { questions: unknown }).questions).toEqual(
      JSON.parse(JSON.stringify(buildArticleQuestions("market"))),
    );
  });

  it("토큰 수가 없으면 0으로 센다", async () => {
    const { fetchImpl } = fakeFetch(() =>
      json(200, answer({ usage: { input_tokens: null, output_tokens: null } })),
    );
    const decision = await decider(fetchImpl).decide(INPUT, "jev-1.13.0");
    expect(decision.usage).toEqual({ inputTokens: 0, outputTokens: 0 });
  });

  it("응답 모양이 다르면(영향도 없음) upstream 오류", async () => {
    const broken = answer();
    const { fetchImpl } = fakeFetch(() =>
      json(200, {
        ...broken,
        answers: {
          relevance: broken.answers.relevance,
          impact: broken.answers.impact,
        },
      }),
    );
    await expect(
      decider(fetchImpl).decide(INPUT, "jev-1.13.0"),
    ).rejects.toMatchObject({
      kind: "upstream",
      message: "Jev 응답 형식이 다릅니다.",
    });
  });

  it("키가 없으면 부르지 않고 no_api_key(이번 회차를 멈추는 오류)", async () => {
    const { fetchImpl, calls } = fakeFetch(() => json(200, answer()));
    const error = await createTypeSafeDecider({
      apiKey: undefined,
      fetchImpl,
    })
      .decide(INPUT, "jev-1.13.0")
      .catch((e: unknown) => e);
    expect(error).toBeInstanceOf(DeciderError);
    expect(error).toMatchObject({ kind: "no_api_key", fatal: true });
    expect(calls).toHaveLength(0);
  });

  it.each([
    [401, "unauthorized", true, 1],
    [403, "unauthorized", true, 1],
    [400, "invalid_request", false, 1],
    // 429·5xx는 SDK가 두 번 더 시도한 뒤 포기한다.
    [429, "rate_limited", false, 3],
    [503, "upstream", false, 3],
  ] as const)(
    "HTTP %i → %s (멈춤 %s, 시도 %i번)",
    async (status, kind, fatal, attempts) => {
      const { fetchImpl, calls } = fakeFetch(() =>
        json(status, { detail: "x" }, { "retry-after": "0" }),
      );
      await expect(
        decider(fetchImpl).decide(INPUT, "jev-1.13.0"),
      ).rejects.toMatchObject({ kind, fatal });
      expect(calls).toHaveLength(attempts);
    },
  );

  it("429 뒤에 성공하면 그 응답을 쓴다", async () => {
    const { fetchImpl, calls } = fakeFetch(
      () => json(429, {}, { "retry-after": "0" }),
      () => json(200, answer()),
    );
    const decision = await decider(fetchImpl).decide(INPUT, "jev-1.13.0");
    expect(decision.impact).toBe("positive");
    expect(calls).toHaveLength(2);
  });
});
