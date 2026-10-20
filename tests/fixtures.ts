import { readFileSync } from "node:fs";
import path from "node:path";
import {
  LlmError,
  type LlmClient,
  type LlmUsage,
  type StructuredRequest,
  type StructuredResult,
} from "../src/server/ai/llm/types";
import type { Db } from "../src/server/db/client";
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

/** 깊이 2 키워드 트리 표본(LLM 출력 형식) */
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
