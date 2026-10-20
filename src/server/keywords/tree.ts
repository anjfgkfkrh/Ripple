import * as z from "zod";
import type { LlmMessage } from "@/server/ai/llm/types";
import type { Market } from "@/server/db/schema";

/** 단계마다 총 5개 (D-13) */
export const KEYWORDS_PER_LEVEL = 5;
export const MIN_DEPTH = 1;
export const MAX_DEPTH = 4;

const MAX_KEYWORD_LENGTH = 40;
const MAX_REASON_LENGTH = 80;
export const PATH_SEPARATOR = " → ";

export function clampDepth(depth: number): number {
  if (!Number.isFinite(depth)) return 2;
  return Math.min(MAX_DEPTH, Math.max(MIN_DEPTH, Math.round(depth)));
}

/** LLM 출력 형식. 부모는 이름으로 가리키는 평평한 목록이고, 트리는 검증하며 만든다. */
export const keywordTreeSchema = z.object({
  keywords: z.array(
    z.object({
      level: z.number().int().describe("1부터 시작하는 단계"),
      keyword: z.string().describe("한국어 키워드(뉴스 검색용 짧은 명사구)"),
      keywordEn: z
        .string()
        .describe(
          "미국 종목: 영어 뉴스 검색용 영어 표기. 한국 종목: 빈 문자열",
        ),
      parent: z
        .string()
        .describe("바로 윗단계 키워드(한국어 그대로). 1단계는 빈 문자열"),
      reason: z
        .string()
        .describe("윗단계(1단계는 종목)와 어떻게 이어지는지 한국어 한 줄"),
    }),
  ),
});
export type RawKeyword = z.infer<typeof keywordTreeSchema>["keywords"][number];

export type KeywordStock = {
  market: Market;
  code: string;
  name: string;
  nameKo: string | null;
  exchange: string;
};

export function stockDisplayName(stock: Pick<KeywordStock, "name" | "nameKo">) {
  return stock.nameKo ?? stock.name;
}

export function buildKeywordMessages(
  stock: KeywordStock,
  depth: number,
): LlmMessage[] {
  const us = stock.market === "US";
  return [
    {
      role: "system",
      content: [
        "너는 주식 투자자를 위한 연관 키워드 설계자다.",
        "종목 하나를 받아, 그 종목의 주가·실적에 영향을 줄 만한 뉴스를 찾기 위한 검색 키워드를 트리로 만든다.",
        `- 키워드 수는 단계마다 모두 합쳐 정확히 ${KEYWORDS_PER_LEVEL}개다. 깊이가 N이면 전체 ${KEYWORDS_PER_LEVEL}×N개다.`,
        `- 1단계: 종목과 직접 이어지는 키워드(제품·사업·고객·경쟁사·원자재·정책 등) ${KEYWORDS_PER_LEVEL}개. parent는 빈 문자열.`,
        `- 2단계부터: 바로 윗단계 키워드 가운데 영향이 큰 것을 골라 그 아래에 이어지는 키워드 ${KEYWORDS_PER_LEVEL}개. parent에는 그 윗단계 키워드를 그대로 적는다. 부모 하나당 ${KEYWORDS_PER_LEVEL}개가 아니라 단계 전체에서 ${KEYWORDS_PER_LEVEL}개이며, 자식이 없는 키워드가 있어도 된다.`,
        "- 요청한 깊이까지만 만든다.",
        "- 키워드는 뉴스 검색에 그대로 쓸 짧은 명사구(1~3단어)로 쓴다. 종목 이름 자체나 지나치게 일반적인 말(주식, 경제, 시장)은 쓰지 않고, 같은 키워드를 두 번 쓰지 않는다.",
        "- keyword는 한국어로 쓴다.",
        us
          ? "- keywordEn에는 영어 뉴스 검색에 쓸 영어 표기를 쓴다."
          : "- keywordEn은 빈 문자열로 둔다.",
        "- reason에는 윗단계(1단계는 종목)와 어떻게 이어지는지 한국어 한 줄(40자 이내)을 쓴다.",
      ].join("\n"),
    },
    {
      role: "user",
      content: [
        `종목: ${stockDisplayName(stock)}${us ? ` / ${stock.name}` : ""} (${us ? "미국" : "한국"}, ${stock.exchange}, ${stock.code})`,
        `깊이: ${depth} (단계마다 ${KEYWORDS_PER_LEVEL}개, 전체 ${KEYWORDS_PER_LEVEL * depth}개)`,
      ].join("\n"),
    },
  ];
}

export type KeywordNode = {
  /** 이 트리 안에서만 쓰는 임시 번호(저장할 때 실제 id로 바뀐다) */
  key: number;
  parentKey: number | null;
  level: number;
  textKo: string;
  textEn: string | null;
  reason: string;
  path: string;
};

/**
 * LLM 출력을 검증해 트리로 만든다. 규칙을 어긴 키워드는 버린다.
 * - 단계는 1부터 depth까지, 단계마다 최대 5개(D-13)
 * - 2단계부터는 바로 윗단계에 실제로 있는 키워드를 부모로 가져야 한다
 * - 같은 키워드·종목 이름 자체는 버린다
 * - 미국 종목만 영어 표기를 남긴다(D-14)
 */
export function buildKeywordTree(
  stock: KeywordStock,
  raw: RawKeyword[],
  depth: number,
): KeywordNode[] {
  const maxLevel = clampDepth(depth);
  const nodes: KeywordNode[] = [];
  const byText = new Map<string, KeywordNode>();
  const forbidden = new Set(
    [stock.name, stock.nameKo, stock.code]
      .filter((value): value is string => Boolean(value))
      .map(normalizeKey),
  );

  for (let level = 1; level <= maxLevel; level++) {
    let count = 0;
    for (const item of raw) {
      if (item.level !== level || count >= KEYWORDS_PER_LEVEL) continue;
      const textKo = clean(item.keyword, MAX_KEYWORD_LENGTH);
      const key = normalizeKey(textKo);
      if (!textKo || forbidden.has(key) || byText.has(key)) continue;

      const parent =
        level === 1
          ? null
          : byText.get(normalizeKey(clean(item.parent, MAX_KEYWORD_LENGTH)));
      if (level > 1 && parent?.level !== level - 1) continue;

      const node: KeywordNode = {
        key: nodes.length + 1,
        parentKey: parent?.key ?? null,
        level,
        textKo,
        textEn:
          stock.market === "US"
            ? clean(item.keywordEn, MAX_KEYWORD_LENGTH) || null
            : null,
        reason: clean(item.reason, MAX_REASON_LENGTH),
        path: [parent?.path ?? stockDisplayName(stock), textKo].join(
          PATH_SEPARATOR,
        ),
      };
      nodes.push(node);
      byText.set(key, node);
      count += 1;
    }
    // 이 단계에 키워드가 하나도 없으면 더 깊은 단계는 이어 붙일 곳이 없다.
    if (count === 0) break;
  }
  return nodes;
}

function clean(text: string, maxLength: number): string {
  return text.replace(/\s+/g, " ").trim().slice(0, maxLength);
}

function normalizeKey(text: string): string {
  return text.replace(/\s+/g, "").toLowerCase();
}
