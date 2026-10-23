import { readFileSync } from "node:fs";
import path from "node:path";
import type {
  Decider,
  Decision,
  DecisionInput,
  LeadInput,
  LeadRating,
} from "../src/server/ai/decider/types";
import {
  LlmError,
  type LlmClient,
  type LlmUsage,
  type StructuredRequest,
  type StructuredResult,
} from "../src/server/ai/llm/types";
import type { Db } from "../src/server/db/client";
import {
  articleMatches,
  articles,
  evaluations,
  type Impact,
  type MatchKind,
} from "../src/server/db/schema";
import type { CollectionRunner } from "../src/server/collect/runner";
import type { RunTrigger } from "../src/server/db/schema";
import type { KeywordQueue } from "../src/server/keywords/queue";
import {
  parseKindList,
  parseNasdaqListed,
  parseOtherListed,
} from "../src/server/listings/parse";
import { replaceListings } from "../src/server/listings/sync";

const FIXTURES = path.resolve("tests/fixtures");

/** 실제 KIND·NASDAQ 파일에서 고른 표본(KIND는 원본처럼 EUC-KR) */
export function fixtureBytes(name: string): Buffer {
  return readFileSync(path.join(FIXTURES, name));
}

export const FIXTURE_FILES = {
  kind: "kind-sample.xls",
  nasdaqListed: "nasdaqlisted-sample.txt",
  otherListed: "otherlisted-sample.txt",
};

/** 표본으로 공식 목록을 채운다(한국 7개, 미국 9개). */
export function seedListings(db: Db): void {
  replaceListings(
    db,
    "KR",
    parseKindList(
      new TextDecoder("euc-kr").decode(fixtureBytes(FIXTURE_FILES.kind)),
    ),
  );
  replaceListings(db, "US", [
    ...parseNasdaqListed(
      fixtureBytes(FIXTURE_FILES.nasdaqListed).toString("utf8"),
    ),
    ...parseOtherListed(
      fixtureBytes(FIXTURE_FILES.otherListed).toString("utf8"),
    ),
  ]);
}

export const FAKE_USAGE: LlmUsage = {
  inputTokens: 500,
  outputTokens: 60,
  costUsd: 0.0001,
};

/** 정해진 값을 돌려주는 가짜 LLM. 받은 요청을 calls에 남긴다. */
export function fakeLlm(
  respond: (request: StructuredRequest<unknown>) => unknown,
  usage: LlmUsage = FAKE_USAGE,
): LlmClient & { calls: StructuredRequest<unknown>[] } {
  const calls: StructuredRequest<unknown>[] = [];
  return {
    provider: "openrouter",
    calls,
    async generateObject<T>(
      request: StructuredRequest<T>,
    ): Promise<StructuredResult<T>> {
      calls.push(request as StructuredRequest<unknown>);
      const parsed = request.schema.safeParse(
        respond(request as StructuredRequest<unknown>),
      );
      if (!parsed.success) {
        throw new LlmError("형식 오류", "invalid_output", usage);
      }
      return { value: parsed.data, usage, model: request.model };
    },
  };
}

/** 사용자 입력에 따라 후보를 돌려주는 가짜 종목 매칭 LLM */
export function fakeStockLlm() {
  return fakeLlm((request) => {
    const query = request.messages.at(-1)?.content ?? "";
    if (query === "삼전") {
      return {
        candidates: [
          {
            market: "KR",
            code: "005930",
            name: "삼성전자",
            nameKo: "삼성전자",
          },
        ],
      };
    }
    if (query === "NVDA" || query === "엔비디아") {
      return {
        candidates: [
          {
            market: "US",
            code: "NVDA",
            name: "NVIDIA Corporation",
            nameKo: "엔비디아",
          },
        ],
      };
    }
    return { candidates: [] };
  });
}

/** 넣은 종목 id만 남기는 가짜 키워드 대기열(키워드를 실제로 만들지 않음) */
export function fakeKeywordQueue(): KeywordQueue & { enqueued: number[] } {
  const enqueued: number[] = [];
  return {
    enqueued,
    enqueue(stockId) {
      enqueued.push(stockId);
    },
    idle: () => Promise.resolve(),
  };
}

/** 깊이 2 키워드 트리 표본(LLM 출력 형식, 구조 키워드만 2026-10-23부터). 단계마다 5개 */
export function sampleKeywordTree(options: { english?: boolean } = {}) {
  const en = (text: string) => (options.english ? text : "");
  const level1 = [
    ["메모리 반도체", "Memory chips", "주력 사업"],
    ["HBM", "HBM", "AI 서버용 고대역폭 메모리"],
    ["스마트폰", "Smartphones", "완제품 사업"],
    ["파운드리", "Foundry", "위탁 생산 사업"],
    ["환율", "Exchange rate", "수출 비중이 커 실적에 영향"],
  ];
  const level2 = [
    ["엔비디아", "Nvidia", "HBM", "HBM 주요 고객"],
    ["SK하이닉스", "SK hynix", "HBM", "HBM 경쟁사"],
    ["D램 가격", "DRAM prices", "메모리 반도체", "메모리 실적을 좌우"],
    ["TSMC", "TSMC", "파운드리", "파운드리 1위 경쟁사"],
    ["애플", "Apple", "스마트폰", "스마트폰 경쟁사"],
  ];
  return {
    keywords: [
      ...level1.map(([keyword, keywordEn, reason]) => ({
        level: 1,
        keyword,
        keywordEn: en(keywordEn),
        parent: "",
        reason,
      })),
      ...level2.map(([keyword, keywordEn, parent, reason]) => ({
        level: 2,
        keyword,
        keywordEn: en(keywordEn),
        parent,
        reason,
      })),
    ],
  };
}

/** 시작 요청만 기록하는 가짜 수집 실행기. finish()를 부르기 전까지는 '수집 중'이다. */
export function fakeCollector(): CollectionRunner & {
  started: RunTrigger[];
  finish(): void;
} {
  const started: RunTrigger[] = [];
  let running: number | null = null;
  return {
    started,
    start(trigger) {
      if (running !== null) return null;
      started.push(trigger);
      running = started.length;
      return running;
    },
    runningId: () => running,
    idle: () => Promise.resolve(),
    finish() {
      running = null;
    },
  };
}

/** 기본 평가 결과(관련 7점·영향 7점·호재·입력 1000토큰) */
export function fakeDecision(overrides: Partial<Decision> = {}): Decision {
  return {
    relevance: 7,
    relevanceConfidence: 0.7,
    magnitude: 7,
    magnitudeConfidence: 0.7,
    impact: "positive",
    impactConfidence: 0.8,
    impactProbabilities: { positive: 0.8, negative: 0.1, neutral: 0.1 },
    model: "jev-1.13.0",
    usage: { inputTokens: 1000, outputTokens: 50 },
    ...overrides,
  };
}

/** 기본 키워드·토픽 평가(관련 7점·영향 6점·입력 500토큰) */
export function fakeRating(overrides: Partial<LeadRating> = {}): LeadRating {
  return {
    relevance: 7,
    relevanceConfidence: 0.7,
    magnitude: 6,
    magnitudeConfidence: 0.7,
    model: "jev-1.13.0",
    usage: { inputTokens: 500, outputTokens: 20 },
    ...overrides,
  };
}

/**
 * 입력마다 정한 결과(또는 오류)를 돌려주는 가짜 Jev. 받은 입력과 모델을 calls(기사 평가)와
 * rateCalls(키워드·토픽 평가, 11단계)에 남긴다.
 */
export function fakeDecider(
  answer: (input: DecisionInput) => Decision | Error = () => fakeDecision(),
  rateAnswer: (input: LeadInput) => LeadRating | Error = () => fakeRating(),
): Decider & {
  calls: { input: DecisionInput; model: string }[];
  rateCalls: { input: LeadInput; model: string }[];
} {
  const calls: { input: DecisionInput; model: string }[] = [];
  const rateCalls: { input: LeadInput; model: string }[] = [];
  return {
    provider: "typesafe",
    calls,
    rateCalls,
    async decide(input, model) {
      calls.push({ input, model });
      const result = answer(input);
      if (result instanceof Error) throw result;
      return result;
    },
    async rate(input, model) {
      rateCalls.push({ input, model });
      const result = rateAnswer(input);
      if (result instanceof Error) throw result;
      return result;
    },
  };
}

let linkSerial = 0;
/**
 * 기사 하나와 종목 연결 하나를 넣는다. relevance를 주면 그 점수(와 impact)로 평가한 것으로 한다.
 * articleId를 주면 그 기사에 연결만 더한다(여러 종목에 걸린 기사).
 */
export function insertLink(
  db: Db,
  link: {
    stockId: number;
    title?: string;
    publishedAt: Date;
    articleId?: number;
    publisher?: string;
    kind?: MatchKind;
    path?: string;
    relevance?: number;
    /** 영향도(11단계). 주지 않으면 11단계 전 평가처럼 비운다 */
    magnitude?: number;
    impact?: Impact;
  },
): { matchId: number; articleId: number } {
  linkSerial += 1;
  const article = link.articleId
    ? { id: link.articleId }
    : db
        .insert(articles)
        .values({
          urlKey: `fixture.example/${linkSerial}`,
          url: `https://fixture.example/${linkSerial}`,
          title: link.title ?? `기사 ${linkSerial}`,
          summary: null,
          publisher: link.publisher ?? "가경제",
          feed: "rss:a:economy",
          publishedAt: link.publishedAt,
          collectedAt: link.publishedAt,
        })
        .returning({ id: articles.id })
        .get();
  const kind = link.kind ?? "name";
  const matchId = db
    .insert(articleMatches)
    .values({
      articleId: article.id,
      stockId: link.stockId,
      keywordPath: link.path ?? "종목",
      keywordReason: null,
      matchKind: kind,
      level: kind === "keyword" ? 1 : 0,
      createdAt: link.publishedAt,
    })
    .returning({ id: articleMatches.id })
    .get().id;
  if (link.relevance !== undefined) {
    db.insert(evaluations)
      .values({
        matchId,
        articleId: article.id,
        stockId: link.stockId,
        relevance: link.relevance,
        relevanceConfidence: 0.7,
        magnitude: link.magnitude ?? null,
        magnitudeConfidence: link.magnitude === undefined ? null : 0.7,
        impact: link.impact ?? "neutral",
        impactConfidence: 0.7,
        impactProbabilities: { positive: 0.1, negative: 0.1, neutral: 0.8 },
        model: "jev-1.13.0",
        evaluatedAt: link.publishedAt,
      })
      .run();
  }
  return { matchId, articleId: article.id };
}
