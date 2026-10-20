import { choice, score } from "@typesafe-ai/sdk";
import type { DecisionInput } from "./types";

/**
 * Jev에 묻는 두 질문(SPEC FR-5). 관련도는 상황 6단계(0~5)로 묻고 ×2 해서 0~10으로 쓴다
 * (Jev Score는 최대 10단계, 2026-10-20 결정). 단계는 '정도'가 아니라 '상황'으로 쓴다(공식 문서 권장).
 * 지시문 언어는 실측으로 고른다: ko(한국어) / en(영어). 기사 내용은 늘 한국어 그대로 보낸다.
 */
export const QUESTION_LANGUAGES = ["ko", "en"] as const;
export type QuestionLanguage = (typeof QUESTION_LANGUAGES)[number];

/**
 * 실제로 쓰는 지시문 언어. 2026-10-20 실측(기사 50건 × 언어 2 × 버전 2)에서 품질은 같고
 * 영어가 입력 토큰이 13% 적어(855 vs 987) 영어로 정했다(SPEC 11장).
 */
export const JEV_QUESTION_LANGUAGE: QuestionLanguage = "en";

/** 0~5단계 기댓값을 화면 점수 0~10으로 바꾸는 배수 */
export const RELEVANCE_SCALE = 2;

const TEXT = {
  ko: {
    relevance:
      "`article`(기사)이 `stock`(종목)의 실적이나 주가와 얼마나 관련이 있는가? `link`는 이 기사가 이 종목에 걸린 경위다.",
    levels: [
      "이 종목과 관련이 없다. 이름이나 키워드가 우연히 겹쳤을 뿐이다.",
      "같은 업계나 시장 전반의 소식이지만 이 종목에 미칠 영향은 거의 없다.",
      "이 종목의 사업·고객·경쟁사·원자재·정책과 이어져 간접적으로 영향을 줄 수 있다.",
      "이 종목의 실적이나 주가에 영향을 줄 만한 구체적인 소식이다.",
      "이 종목이 직접 나오는 중요한 소식이다(실적, 수주, 투자, 규제, 소송, 경영 변화 등).",
      "이 종목의 주가를 크게 움직일 만한 핵심 소식이다.",
    ],
    impact:
      "`article`(기사)은 `stock`(종목)의 주가에 어떤 영향을 주는 소식인가?",
    impacts: {
      positive: "이 종목의 실적이나 주가에 좋은 소식(호재)",
      negative: "이 종목의 실적이나 주가에 나쁜 소식(악재)",
      neutral: "영향이 없거나 방향이 분명하지 않은 소식(중립)",
    },
    linkKinds: {
      direct: "뉴스 제공처가 이 기사를 이 종목의 뉴스로 분류했다.",
      name: "기사에 종목 이름이 나온다.",
      keyword:
        "기사에 종목과 이어진 키워드가 나온다. path는 종목에서 키워드까지의 연결, reason은 그 키워드가 윗단계와 이어지는 이유다.",
    },
  },
  en: {
    relevance:
      "How relevant is the `article` to the earnings or share price of the company in `stock`? `link` explains how the article was matched to the company.",
    levels: [
      "Unrelated to this company; the name or keyword match is a coincidence.",
      "General industry or market news with almost no effect on this company.",
      "Connected to this company's business, customers, competitors, materials, or regulation, so it could affect the company indirectly.",
      "Concrete news that could affect this company's earnings or share price.",
      "Important news in which this company itself appears (earnings, orders, investment, regulation, lawsuits, management changes).",
      "Key news likely to move this company's share price substantially.",
    ],
    impact:
      "What effect does the `article` have on the share price of the company in `stock`?",
    impacts: {
      positive: "Good news for this company's earnings or share price",
      negative: "Bad news for this company's earnings or share price",
      neutral: "No effect, or the direction is unclear",
    },
    linkKinds: {
      direct:
        "The news provider tagged this article as news about this company.",
      name: "The article mentions the company name.",
      keyword:
        "The article mentions a keyword related to the company. path is the chain from the company to the keyword; reason explains how the keyword relates to the previous step.",
    },
  },
} as const;

export function buildQuestions(language: QuestionLanguage) {
  const t = TEXT[language];
  return {
    relevance: score(t.relevance, t.levels),
    impact: choice(t.impact, t.impacts),
  };
}

/** Jev의 state: 종목, 걸린 경위, 기사(제목+요약). 판단에 필요 없는 것(날짜 등)은 넣지 않는다. */
export function buildState(input: DecisionInput, language: QuestionLanguage) {
  const { stock, link, article } = input;
  return {
    stock: {
      name: stock.name,
      ...(stock.officialName && stock.officialName !== stock.name
        ? { official_name: stock.officialName }
        : {}),
      market: stock.market === "KR" ? "Korea" : "United States",
      exchange: stock.exchange,
      code: stock.code,
    },
    link: {
      how: TEXT[language].linkKinds[link.kind],
      path: link.path,
      ...(link.reason ? { reason: link.reason } : {}),
    },
    article: {
      title: article.title,
      ...(article.summary ? { summary: article.summary } : {}),
    },
  };
}
