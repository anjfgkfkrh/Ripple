// Jev 질문·state 문구를 TS가 만든 그대로 파일로 내보낸다(docs/RUST_MIGRATION.md 6장 R2: 질문 문구가 글자 하나까지 같음).
// Rust 테스트가 같은 입력으로 만든 값과 이 파일을 글자로 비교한다.
// 실행: pnpm exec tsx rust/tools/dump-questions.ts [출력 파일]
import { writeFileSync } from "node:fs";
import path from "node:path";
import {
  buildArticleQuestions,
  buildArticleState,
  buildLeadQuestions,
  buildLeadState,
} from "../../src/server/ai/decider/questions";
import {
  JUDGE_TARGETS,
  type DecisionInput,
  type JudgeItem,
  type LeadInput,
} from "../../src/server/ai/decider/types";

const SAMSUNG: JudgeItem = {
  kind: "stock",
  name: "삼성전자",
  officialName: null,
  market: "KR",
  exchange: "KOSPI",
  code: "005930",
};
const NVIDIA: JudgeItem = {
  kind: "stock",
  name: "엔비디아",
  officialName: "NVIDIA Corporation - Common Stock",
  market: "US",
  exchange: "NASDAQ",
  code: "NVDA",
};
const SAME_NAME: JudgeItem = { ...NVIDIA, officialName: "엔비디아" };
const KR_MARKET: JudgeItem = {
  kind: "market",
  name: "한국 증시",
  officialName: null,
  market: "KR",
  exchange: "KOSPI·KOSDAQ",
  code: "MARKET",
};
const TOPIC: JudgeItem = {
  kind: "topic",
  name: "중동 긴장",
  officialName: null,
  market: "US",
  exchange: "미국 증시",
  code: "TOPIC-1",
  summary: "사우디 시설 공격 위협",
};
const TOPIC_NO_SUMMARY: JudgeItem = { ...TOPIC, summary: null };

const keywordLink = {
  kind: "keyword" as const,
  path: "삼성전자 → HBM 메모리",
  reason: "주력 메모리 제품",
};
const articles: DecisionInput[] = [
  {
    stock: SAMSUNG,
    link: keywordLink,
    article: {
      title: "HBM 증설 경쟁",
      summary: "메모리 업계가 증설에 나섰다.",
    },
  },
  {
    stock: NVIDIA,
    link: { kind: "direct", path: "엔비디아", reason: null },
    article: { title: "Nvidia climbs", summary: null },
  },
  {
    stock: SAME_NAME,
    link: { kind: "name", path: "엔비디아", reason: "" },
    article: { title: "엔비디아 실적", summary: "" },
  },
  {
    stock: KR_MARKET,
    link: { kind: "keyword", path: "한국 증시 → 기준금리", reason: null },
    article: { title: "기준금리 동결", summary: null },
  },
  {
    stock: TOPIC,
    link: { kind: "keyword", path: "중동 긴장 → 유가", reason: "공급 불안" },
    article: { title: "유가 급등", summary: "브렌트유 상승" },
  },
  {
    stock: TOPIC_NO_SUMMARY,
    link: { kind: "name", path: "중동 긴장", reason: null },
    article: { title: "중동 정세", summary: null },
  },
];
const leads: LeadInput[] = [
  {
    stock: SAMSUNG,
    subject: {
      type: "keyword",
      text: "엔비디아",
      path: "삼성전자 → HBM → 엔비디아",
      reason: "HBM 주요 고객",
    },
  },
  {
    stock: SAMSUNG,
    subject: {
      type: "topic",
      text: "미중 반도체 갈등",
      summary: "미국이 수출 통제를 넓힘",
      terms: ["수출 통제", "반도체 관세"],
      reason: null,
    },
  },
  {
    stock: TOPIC,
    subject: { type: "topic", text: "금리", terms: [], reason: "" },
  },
];

const dump = {
  articleQuestions: Object.fromEntries(
    JUDGE_TARGETS.map((t) => [t, buildArticleQuestions(t)]),
  ),
  leadQuestions: Object.fromEntries(
    JUDGE_TARGETS.map((t) => [t, buildLeadQuestions(t)]),
  ),
  articles: articles.map((input) => ({
    input,
    state: buildArticleState(input),
  })),
  leads: leads.map((input) => ({ input, state: buildLeadState(input) })),
};

const out = path.resolve(
  process.argv[2] ?? "rust/ripple/src/server/ai/decider/questions_fixture.json",
);
writeFileSync(out, `${JSON.stringify(dump, null, 2)}\n`);
console.log(out);
