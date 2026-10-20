import { describe, expect, it } from "vitest";
import type { DecisionInput } from "./types";
import {
  buildQuestions,
  buildState,
  JEV_QUESTION_LANGUAGE,
  QUESTION_LANGUAGES,
} from "./questions";

const SAMSUNG_KEYWORD: DecisionInput = {
  stock: {
    name: "삼성전자",
    officialName: null,
    market: "KR",
    exchange: "KOSPI",
    code: "005930",
  },
  link: {
    kind: "keyword",
    path: "삼성전자 → HBM 메모리",
    reason: "주력 메모리 제품",
  },
  article: { title: "HBM 증설 경쟁", summary: "메모리 업계가 증설에 나섰다." },
};

describe("buildQuestions (SPEC FR-5)", () => {
  it.each(QUESTION_LANGUAGES)(
    "%s: 관련도는 상황 6단계 점수, 영향은 호재·악재·중립 선택",
    (language) => {
      const { relevance, impact } = buildQuestions(language);
      expect(relevance.type).toBe("score");
      expect(relevance.criteria).toHaveLength(6);
      expect(impact.type).toBe("choice");
      expect(Object.keys(impact.criteria)).toEqual([
        "positive",
        "negative",
        "neutral",
      ]);
    },
  );

  it("실측으로 정한 지시문 언어는 영어다(2026-10-20)", () => {
    expect(JEV_QUESTION_LANGUAGE).toBe("en");
  });
});

describe("buildState (D-21)", () => {
  it("종목·걸린 경위(경로와 이유)·제목과 요약만 넣는다", () => {
    expect(buildState(SAMSUNG_KEYWORD, "en")).toEqual({
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
    const state = buildState(
      {
        stock: {
          name: "엔비디아",
          officialName: "NVIDIA Corporation - Common Stock",
          market: "US",
          exchange: "NASDAQ",
          code: "NVDA",
        },
        link: { kind: "direct", path: "엔비디아", reason: null },
        article: { title: "Nvidia climbs", summary: null },
      },
      "ko",
    );
    expect(state.stock).toEqual({
      name: "엔비디아",
      official_name: "NVIDIA Corporation - Common Stock",
      market: "United States",
      exchange: "NASDAQ",
      code: "NVDA",
    });
    expect(state.link).toEqual({
      how: "뉴스 제공처가 이 기사를 이 종목의 뉴스로 분류했다.",
      path: "엔비디아",
    });
    expect(state.article).toEqual({ title: "Nvidia climbs" });
  });
});
