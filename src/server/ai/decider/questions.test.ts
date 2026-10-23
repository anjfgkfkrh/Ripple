import { describe, expect, it } from "vitest";
import type { DecisionInput, JudgeItem } from "./types";
import { JUDGE_TARGETS } from "./types";
import {
  buildArticleQuestions,
  buildArticleState,
  buildLeadQuestions,
  buildLeadState,
} from "./questions";

const SAMSUNG: JudgeItem = {
  kind: "stock",
  name: "삼성전자",
  officialName: null,
  market: "KR",
  exchange: "KOSPI",
  code: "005930",
};

const SAMSUNG_KEYWORD: DecisionInput = {
  stock: SAMSUNG,
  link: {
    kind: "keyword",
    path: "삼성전자 → HBM 메모리",
    reason: "주력 메모리 제품",
  },
  article: { title: "HBM 증설 경쟁", summary: "메모리 업계가 증설에 나섰다." },
};

const KR_MARKET: JudgeItem = {
  kind: "market",
  name: "한국 증시",
  officialName: null,
  market: "KR",
  exchange: "KOSPI·KOSDAQ",
  code: "MARKET",
};

describe("buildArticleQuestions (SPEC FR-5, 11단계)", () => {
  it.each(JUDGE_TARGETS)(
    "%s: 관련도·영향도는 상황 6단계 점수, 영향은 호재·악재·중립 선택",
    (target) => {
      const { relevance, magnitude, impact } = buildArticleQuestions(target);
      expect(relevance.type).toBe("score");
      expect(relevance.criteria).toHaveLength(6);
      expect(magnitude.type).toBe("score");
      expect(magnitude.criteria).toHaveLength(6);
      expect(impact.type).toBe("choice");
      expect(Object.keys(impact.criteria)).toEqual([
        "positive",
        "negative",
        "neutral",
      ]);
    },
  );

  it("종목은 그 종목을, 시장·주제는 증시 전체를 움직이는지 묻는다(주제는 증시 기준)", () => {
    expect(buildArticleQuestions("stock").magnitude.instructions).toContain(
      "company in `stock`",
    );
    for (const target of ["market", "topic"] as const) {
      const { magnitude, impact } = buildArticleQuestions(target);
      expect(magnitude.instructions).toContain("overall stock market");
      expect(impact.instructions).toContain("overall stock market");
    }
    expect(buildArticleQuestions("topic").relevance.instructions).toContain(
      "topic in `topic`",
    );
  });
});

describe("buildLeadQuestions (11단계)", () => {
  it.each(JUDGE_TARGETS)("%s: 관련도·영향도 두 점수만 묻는다", (target) => {
    const questions = buildLeadQuestions(target);
    expect(Object.keys(questions)).toEqual(["relevance", "magnitude"]);
    expect(questions.relevance.criteria).toHaveLength(6);
    expect(questions.magnitude.criteria).toHaveLength(6);
  });
});

describe("buildArticleState (D-21)", () => {
  it("종목·걸린 경위(경로와 이유)·제목과 요약만 넣는다", () => {
    expect(buildArticleState(SAMSUNG_KEYWORD)).toEqual({
      stock: {
        name: "삼성전자",
        market: "Korea",
        exchange: "KOSPI",
        code: "005930",
      },
      link: {
        how: expect.stringContaining("keyword"),
        path: "삼성전자 → HBM 메모리",
        reason: "주력 메모리 제품",
      },
      article: {
        title: "HBM 증설 경쟁",
        summary: "메모리 업계가 증설에 나섰다.",
      },
    });
  });

  it("미국 종목은 공식 영문 이름을 함께 넣고, 없는 이유·요약은 뺀다", () => {
    const state = buildArticleState({
      stock: {
        kind: "stock",
        name: "엔비디아",
        officialName: "NVIDIA Corporation - Common Stock",
        market: "US",
        exchange: "NASDAQ",
        code: "NVDA",
      },
      link: { kind: "direct", path: "엔비디아", reason: null },
      article: { title: "Nvidia climbs", summary: null },
    });
    expect(state.stock).toEqual({
      name: "엔비디아",
      official_name: "NVIDIA Corporation - Common Stock",
      market: "United States",
      exchange: "NASDAQ",
      code: "NVDA",
    });
    expect(state.link).toEqual({
      how: "The news provider tagged this article as news about this company.",
      path: "엔비디아",
    });
    expect(state.article).toEqual({ title: "Nvidia climbs" });
  });

  it("시장은 증시 이름만, 주제는 주제(설명)와 기준 증시를 넣는다(11단계)", () => {
    const article = { title: "기준금리 동결", summary: null };
    const link = {
      kind: "keyword" as const,
      path: "한국 증시 → 기준금리",
      reason: null,
    };
    expect(buildArticleState({ stock: KR_MARKET, link, article })).toEqual({
      market: { name: "Korean stock market (KOSPI, KOSDAQ)" },
      link: { how: expect.any(String), path: "한국 증시 → 기준금리" },
      article: { title: "기준금리 동결" },
    });
    const topic = buildArticleState({
      stock: {
        kind: "topic",
        name: "중동 긴장",
        officialName: null,
        market: "US",
        exchange: "미국 증시",
        code: "TOPIC-1",
        summary: "사우디 시설 공격 위협",
      },
      link,
      article,
    });
    expect(topic.topic).toEqual({
      name: "중동 긴장",
      summary: "사우디 시설 공격 위협",
    });
    expect(topic.market).toEqual({ name: "US stock market (S&P 500, Nasdaq)" });
    expect(topic.stock).toBeUndefined();
  });
});

describe("buildLeadState (11단계)", () => {
  it("키워드는 경로와 이유를, 토픽은 설명과 검색어를 넣는다", () => {
    expect(
      buildLeadState({
        stock: SAMSUNG,
        subject: {
          type: "keyword",
          text: "엔비디아",
          path: "삼성전자 → HBM → 엔비디아",
          reason: "HBM 주요 고객",
        },
      }).subject,
    ).toEqual({
      type: "search keyword",
      text: "엔비디아",
      path: "삼성전자 → HBM → 엔비디아",
      reason: "HBM 주요 고객",
    });
    expect(
      buildLeadState({
        stock: SAMSUNG,
        subject: {
          type: "topic",
          text: "미중 반도체 갈등",
          summary: "미국이 수출 통제를 넓힘",
          terms: ["수출 통제", "반도체 관세"],
          reason: null,
        },
      }).subject,
    ).toEqual({
      type: "market topic",
      text: "미중 반도체 갈등",
      summary: "미국이 수출 통제를 넓힘",
      search_terms: ["수출 통제", "반도체 관세"],
    });
  });
});
