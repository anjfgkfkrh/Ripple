// TS가 만드는 LLM 프롬프트를 파일로 내보낸다(docs/RUST_MIGRATION.md 6장: 프롬프트 글자 비교 테스트).
// Rust 테스트가 같은 입력으로 만든 프롬프트와 이 파일을 글자로 비교한다. 하위 단계(R4·R7)에서 항목을 더한다.
// 실행: pnpm exec tsx rust/tools/dump-prompts.ts [출력 폴더]
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import {
  buildKeywordMessages,
  type KeywordStock,
} from "../../src/server/keywords/tree";
import { buildMatchMessages } from "../../src/server/stocks/match";

const outDir = path.resolve(
  process.argv[2] ?? "rust/ripple/src/server/prompts",
);
mkdirSync(outDir, { recursive: true });

const SAMSUNG: KeywordStock = {
  market: "KR",
  code: "005930",
  name: "삼성전자",
  nameKo: "삼성전자",
  exchange: "KOSPI",
};
const NVIDIA: KeywordStock = {
  market: "US",
  code: "NVDA",
  name: "NVIDIA Corporation",
  nameKo: "엔비디아",
  exchange: "NASDAQ",
};
const marketItem = (market: "KR" | "US"): KeywordStock => ({
  kind: "market",
  market,
  code: "MARKET",
  name: market === "KR" ? "한국 증시" : "미국 증시",
  nameKo: market === "KR" ? "한국 증시" : "미국 증시",
  exchange: market === "KR" ? "KOSPI·KOSDAQ" : "S&P 500·NASDAQ",
});
const topicItem = (market: "KR" | "US"): KeywordStock => ({
  kind: "topic",
  market,
  code: "TOPIC-1",
  name: "부동산 시장",
  nameKo: "부동산 시장",
  exchange: market === "KR" ? "한국 증시" : "미국 증시",
});
const HEADLINES = ["삼성전자 노조 협상", "삼성전자, 인도 스마트폰 1위"];
const keywordCases: {
  stock: KeywordStock;
  depth: number;
  headlines?: string[];
}[] = [
  { stock: SAMSUNG, depth: 2 },
  { stock: NVIDIA, depth: 3 },
  { stock: SAMSUNG, depth: 2, headlines: HEADLINES },
  { stock: NVIDIA, depth: 4, headlines: HEADLINES },
  { stock: marketItem("KR"), depth: 3 },
  { stock: marketItem("US"), depth: 2 },
  { stock: topicItem("KR"), depth: 2 },
  { stock: topicItem("US"), depth: 1, headlines: [] },
  { stock: { ...SAMSUNG, nameKo: null }, depth: 2 },
];

const FILES: Record<string, unknown> = {
  "keywords.json": keywordCases.map((c) => ({
    stock: c.stock,
    depth: c.depth,
    headlines: c.headlines ?? null,
    messages: buildKeywordMessages(
      c.stock,
      c.depth,
      c.headlines ? { headlines: c.headlines } : undefined,
    ),
  })),
  "match.json": ["삼전", "NVDA", "  공백 있는 입력  "].map((query) => ({
    query,
    messages: buildMatchMessages(query),
  })),
};
for (const [name, value] of Object.entries(FILES)) {
  writeFileSync(path.join(outDir, name), `${JSON.stringify(value, null, 2)}\n`);
  console.log(name);
}
