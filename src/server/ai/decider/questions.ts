import { choice, score } from "@typesafe-ai/sdk";
import type { DecisionInput, JudgeTarget, LeadInput } from "./types";

/**
 * Jev에 묻는 질문(SPEC FR-5, 11단계 결정). 판단을 Jev 한 곳에 모은다.
 * - 기사: 관련도(얼마나 이 항목 얘기인가)·영향도(주가·시장을 얼마나 움직일까)·호재/악재를 한 요청에 묻는다.
 * - 키워드·토픽(트리 후보, 토픽×종목 쌍): 관련도·영향도를 묻는다.
 * 점수는 상황 6단계(0~5)로 묻고 ×2 해서 0~10으로 쓴다(Jev Score는 최대 10단계, 2026-10-20 결정).
 * 단계는 '정도'가 아니라 '상황'으로 쓴다(공식 문서 권장). 지시문은 영어(2026-10-20 실측: 품질은 같고
 * 입력 토큰이 적음), 기사·키워드 내용은 한국어 그대로 보낸다.
 * 대상은 종목(stock)·시장(market: 한국·미국 증시)·주제(topic: 사용자 주제·자동 토픽, 증시 기준으로 판단)다.
 */

/** 0~5단계 기댓값을 화면 점수 0~10으로 바꾸는 배수 */
export const RELEVANCE_SCALE = 2;

/** 판단 대상을 질문에서 부르는 말 */
const TARGET = {
  stock: "the company in `stock`",
  market: "the overall stock market in `market`",
  topic: "the topic in `topic`",
} as const satisfies Record<JudgeTarget, string>;

/** 영향도는 종목이면 그 종목, 시장·주제면 증시 전체(주제는 증시 기준, 11단계 결정) */
const MOVES = {
  stock: "the earnings or share price of the company in `stock`",
  market: "the overall stock market in `market`",
  topic: "the overall stock market in `market`",
} as const satisfies Record<JudgeTarget, string>;

const ARTICLE_RELEVANCE_LEVELS = {
  stock: [
    "Unrelated to this company; the name or keyword match is a coincidence.",
    "Touches this company's field only in passing.",
    "About this company's industry or the wider economy, not this company in particular.",
    "About something connected to this company's business: its products, customers, competitors, suppliers, materials, or rules that apply to it.",
    "About this company together with other companies.",
    "Mainly about this company itself.",
  ],
  market: [
    "Unrelated to the stock market.",
    "About a single company or a narrow niche, with little to do with the market as a whole.",
    "About one industry or sector.",
    "About an economy-wide factor (interest rates, currency, prices, trade, policy, geopolitics) or several large sectors.",
    "About the direction of the market itself, or a factor the whole market is watching right now.",
    "Central to the whole market right now.",
  ],
  topic: [
    "Unrelated to the topic; the keyword match is a coincidence.",
    "Mentions the topic only in passing.",
    "Background or commentary about the topic.",
    "A concrete development in the topic.",
    "An important development in the topic.",
    "The central news of the topic.",
  ],
} as const satisfies Record<JudgeTarget, readonly string[]>;

const STOCK_MAGNITUDE_LEVELS = [
  "No effect on this company's earnings or share price.",
  "A negligible effect.",
  "A small or short-lived effect.",
  "A noticeable effect, such as a move of a few percent or a change to one quarter's results.",
  "A significant effect that changes the outlook, such as a major order, a guidance change, or a new policy.",
  "A major effect likely to move the share price substantially.",
] as const;

const MARKET_MAGNITUDE_LEVELS = [
  "No effect on the overall market.",
  "A negligible effect.",
  "A small or short-lived effect on some sectors.",
  "A noticeable effect on the market or several large sectors for a day or two.",
  "A significant effect that changes market expectations, such as an interest-rate decision or a major policy or geopolitical event.",
  "A major effect likely to move the whole market substantially.",
] as const;

const MAGNITUDE_LEVELS = {
  stock: STOCK_MAGNITUDE_LEVELS,
  market: MARKET_MAGNITUDE_LEVELS,
  topic: MARKET_MAGNITUDE_LEVELS,
} as const;

/** 트리 후보 키워드·토픽이 대상과 얼마나 이어지는가 */
const LEAD_RELEVANCE_LEVELS = [
  "Not connected.",
  "Connected only through several unlikely steps.",
  "Indirectly connected through the wider economy or industry.",
  "Clearly connected through the business, customers, competitors, inputs, or rules involved.",
  "A direct driver.",
  "Core to it, or a direct catalyst.",
] as const;

const LINK_KINDS = {
  direct: "The news provider tagged this article as news about this company.",
  name: "The article mentions the company name.",
  keyword:
    "The article mentions a search keyword. path is the chain from the target to the keyword; reason explains how the keyword relates to the previous step.",
} as const;

/** 기사 질문: 관련도·영향도(점수)와 호재/악재(선택) */
export function buildArticleQuestions(target: JudgeTarget) {
  return {
    relevance: score(
      `How directly is the \`article\` about ${TARGET[target]}? \`link\` explains how the article was matched.`,
      ARTICLE_RELEVANCE_LEVELS[target],
    ),
    magnitude: score(
      `How much could the news in the \`article\` move ${MOVES[target]}?`,
      MAGNITUDE_LEVELS[target],
    ),
    impact: choice(
      `What effect does the \`article\` have on ${MOVES[target]}?`,
      {
        positive: "Good news",
        negative: "Bad news",
        neutral: "No effect, or the direction is unclear",
      },
    ),
  };
}

/** 키워드·토픽 질문: 트리에 남길지, 종목에 붙일지 판단하는 관련도·영향도(점수) */
export function buildLeadQuestions(target: JudgeTarget) {
  return {
    relevance: score(
      `How closely is news about the \`subject\` connected to ${TARGET[target]}? \`subject.reason\` explains the connection.`,
      LEAD_RELEVANCE_LEVELS,
    ),
    magnitude: score(
      `If there is important news about the \`subject\`, how much could it move ${MOVES[target]}?`,
      MAGNITUDE_LEVELS[target],
    ),
  };
}

type StateObject = { [key: string]: string | string[] };

/** 대상 항목을 state에 넣는 모양: 종목은 stock, 시장은 market, 주제는 topic과 기준 증시(market) */
function targetState(
  item: DecisionInput["stock"],
): Record<string, StateObject> {
  const marketName =
    item.market === "KR"
      ? "Korean stock market (KOSPI, KOSDAQ)"
      : "US stock market (S&P 500, Nasdaq)";
  switch (item.kind) {
    case "market":
      return { market: { name: marketName } };
    case "topic":
      return {
        topic: {
          name: item.name,
          ...(item.summary ? { summary: item.summary } : {}),
        },
        market: { name: marketName },
      };
    default:
      return {
        stock: {
          name: item.name,
          ...(item.officialName && item.officialName !== item.name
            ? { official_name: item.officialName }
            : {}),
          market: item.market === "KR" ? "Korea" : "United States",
          exchange: item.exchange,
          code: item.code,
        },
      };
  }
}

/** 기사 평가 state: 대상, 걸린 경위, 기사(제목+요약). 판단에 필요 없는 것(날짜 등)은 넣지 않는다(D-21). */
export function buildArticleState(
  input: DecisionInput,
): Record<string, StateObject> {
  const { stock, link, article } = input;
  return {
    ...targetState(stock),
    link: {
      how: LINK_KINDS[link.kind],
      path: link.path,
      ...(link.reason ? { reason: link.reason } : {}),
    },
    article: {
      title: article.title,
      ...(article.summary ? { summary: article.summary } : {}),
    },
  };
}

/** 키워드·토픽 평가 state: 대상과 판단할 것(키워드면 경로, 토픽이면 설명·검색어)과 이어지는 이유 */
export function buildLeadState(input: LeadInput): Record<string, StateObject> {
  const { subject } = input;
  return {
    ...targetState(input.stock),
    subject: {
      type: subject.type === "keyword" ? "search keyword" : "market topic",
      text: subject.text,
      ...(subject.path ? { path: subject.path } : {}),
      ...(subject.summary ? { summary: subject.summary } : {}),
      ...(subject.terms && subject.terms.length > 0
        ? { search_terms: subject.terms }
        : {}),
      ...(subject.reason ? { reason: subject.reason } : {}),
    },
  };
}
