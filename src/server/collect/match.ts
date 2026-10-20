import type { Market, MatchKind } from "@/server/db/schema";

/** 기사와 맞춰 볼 종목: 종목 이름과 키워드 트리 */
export type MatchStock = {
  id: number;
  market: Market;
  code: string;
  name: string;
  nameKo: string | null;
  keywords: {
    level: number;
    textKo: string;
    textEn: string | null;
    path: string;
    reason: string;
  }[];
};

export type StockMatch = {
  stockId: number;
  /** D-16: 당시 경로를 글자로 남긴다. 종목 자체로 걸리면 종목 이름 */
  keywordPath: string;
  /** 키워드 연결일 때 그 키워드의 이유(5단계 Jev 입력) */
  keywordReason: string | null;
  matchKind: MatchKind;
  /** 0이면 종목 자체, 1~4는 키워드 단계 */
  level: number;
};

type Term = {
  words: string[];
  level: number;
  path: string;
  reason: string | null;
  kind: MatchKind;
};

/**
 * 제목+요약에 종목 이름이나 키워드의 낱말이 모두 들어 있으면 그 종목에 건다(4단계 결정).
 * 종목마다 가장 가까운 경로 하나만 고른다: 종목 이름 > 1단계 > 2단계 …(같은 단계는 먼저 만든 키워드).
 * 영문·숫자 낱말은 낱말 경계를 지켜 찾는다("AI"가 "said"에 걸리지 않게).
 */
export function createMatcher(
  stocks: MatchStock[],
): (text: string) => StockMatch[] {
  const prepared = stocks.map((stock) => {
    const display = stock.nameKo ?? stock.name;
    const terms: Term[] = [
      {
        words: toWords(display),
        level: 0,
        path: display,
        reason: null,
        kind: "name",
      },
    ];
    for (const keyword of stock.keywords) {
      const base = {
        level: keyword.level,
        path: keyword.path,
        reason: keyword.reason || null,
        kind: "keyword" as const,
      };
      terms.push({ ...base, words: toWords(keyword.textKo) });
      if (keyword.textEn) {
        terms.push({ ...base, words: toWords(keyword.textEn) });
      }
    }
    // 가까운 경로가 먼저 오게 한다(정렬은 안정적이라 같은 단계는 원래 순서).
    terms.sort((a, b) => a.level - b.level);
    return { id: stock.id, terms: terms.filter((t) => t.words.length > 0) };
  });

  return (text) => {
    const haystack = normalize(text);
    const matches: StockMatch[] = [];
    for (const stock of prepared) {
      const hit = stock.terms.find((term) =>
        term.words.every((word) => containsWord(haystack, word)),
      );
      if (hit) {
        matches.push({
          stockId: stock.id,
          keywordPath: hit.path,
          keywordReason: hit.reason,
          matchKind: hit.kind,
          level: hit.level,
        });
      }
    }
    return matches;
  };
}

function normalize(text: string): string {
  return text.toLowerCase().replace(/\s+/g, " ");
}

function toWords(text: string): string[] {
  return normalize(text)
    .split(" ")
    .map((word) => word.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, ""))
    .filter(Boolean);
}

const ASCII_WORD = /^[a-z0-9][a-z0-9.&+'-]*$/;

function containsWord(haystack: string, word: string): boolean {
  if (!ASCII_WORD.test(word)) return haystack.includes(word);
  const escaped = word.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`(?<![a-z0-9])${escaped}(?![a-z0-9])`).test(haystack);
}
