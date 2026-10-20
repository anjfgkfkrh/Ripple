import { describe, expect, it } from "vitest";
import { createMatcher, type MatchStock } from "./match";

const SAMSUNG: MatchStock = {
  id: 1,
  market: "KR",
  code: "005930",
  name: "삼성전자",
  nameKo: "삼성전자",
  keywords: [
    {
      level: 1,
      textKo: "HBM 메모리",
      textEn: null,
      path: "삼성전자 → HBM 메모리",
    },
    {
      level: 1,
      textKo: "파운드리 수주",
      textEn: null,
      path: "삼성전자 → 파운드리 수주",
    },
    {
      level: 2,
      textKo: "엔비디아",
      textEn: null,
      path: "삼성전자 → HBM 메모리 → 엔비디아",
    },
  ],
};
const NVIDIA: MatchStock = {
  id: 2,
  market: "US",
  code: "NVDA",
  name: "NVIDIA Corporation - Common Stock",
  nameKo: "엔비디아",
  keywords: [
    {
      level: 1,
      textKo: "AI 가속기",
      textEn: "AI accelerator",
      path: "엔비디아 → AI 가속기",
    },
    {
      level: 2,
      textKo: "HBM 메모리",
      textEn: "HBM memory",
      path: "엔비디아 → AI 가속기 → HBM 메모리",
    },
  ],
};

const match = createMatcher([SAMSUNG, NVIDIA]);

describe("createMatcher (4단계 결정: 낱말이 모두 들어 있으면 연결)", () => {
  it("종목 이름이 있으면 종목 자체(0단계)로 건다", () => {
    expect(match("삼성전자, 3분기 실적 발표")).toEqual([
      { stockId: 1, keywordPath: "삼성전자", matchKind: "name", level: 0 },
    ]);
  });

  it("키워드의 낱말이 떨어져 있거나 조사가 붙어도 모두 있으면 건다", () => {
    expect(match("메모리 가격 급등에 HBM이 견인")).toEqual([
      {
        stockId: 1,
        keywordPath: "삼성전자 → HBM 메모리",
        matchKind: "keyword",
        level: 1,
      },
      {
        stockId: 2,
        keywordPath: "엔비디아 → AI 가속기 → HBM 메모리",
        matchKind: "keyword",
        level: 2,
      },
    ]);
    // 낱말 하나만 있으면 걸지 않는다.
    expect(match("메모리 시장 전망")).toEqual([]);
  });

  it("종목마다 가장 가까운 경로 하나: 종목 이름 > 1단계 > 2단계", () => {
    // '엔비디아'는 삼성전자의 2단계이자 엔비디아 종목 이름이다.
    expect(match("엔비디아 HBM 메모리 주문")).toEqual([
      {
        stockId: 1,
        keywordPath: "삼성전자 → HBM 메모리",
        matchKind: "keyword",
        level: 1,
      },
      { stockId: 2, keywordPath: "엔비디아", matchKind: "name", level: 0 },
    ]);
  });

  it("영어 키워드도 찾고, 영문 낱말은 낱말 경계를 지킨다", () => {
    expect(match("Demand for the new AI accelerator soars")).toEqual([
      {
        stockId: 2,
        keywordPath: "엔비디아 → AI 가속기",
        matchKind: "keyword",
        level: 1,
      },
    ]);
    // 'ai'가 'said', 'accelerators'의 일부로 걸리면 안 된다.
    expect(match("He said accelerators are hot")).toEqual([]);
    // 한글 조사가 붙은 영문은 찾는다.
    expect(match("AI가 가속기 시장을 키운다")).toEqual([
      {
        stockId: 2,
        keywordPath: "엔비디아 → AI 가속기",
        matchKind: "keyword",
        level: 1,
      },
    ]);
  });

  it("대소문자와 공백은 무시한다", () => {
    expect(match("hbm   메모리")).toHaveLength(2);
  });

  it("키워드가 없는 종목은 이름으로만 건다", () => {
    const onlyName = createMatcher([{ ...SAMSUNG, keywords: [] }]);
    expect(onlyName("HBM 메모리")).toEqual([]);
    expect(onlyName("삼성전자")).toHaveLength(1);
  });
});
