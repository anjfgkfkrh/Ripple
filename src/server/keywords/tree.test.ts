import { describe, expect, it } from "vitest";
import { sampleKeywordTree } from "../../../tests/fixtures";
import {
  buildKeywordMessages,
  buildKeywordTree,
  clampDepth,
  keywordTreeSchema,
  type KeywordStock,
  type RawKeyword,
} from "./tree";

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

const raw = (
  level: number,
  keyword: string,
  parent = "",
  extra: Partial<RawKeyword> = {},
): RawKeyword => ({
  level,
  keyword,
  keywordEn: "",
  parent,
  reason: `${keyword} 이유`,
  ...extra,
});

describe("clampDepth", () => {
  it("1~4로 맞추고, 숫자가 아니면 기본 2", () => {
    expect(clampDepth(0)).toBe(1);
    expect(clampDepth(2)).toBe(2);
    expect(clampDepth(2.4)).toBe(2);
    expect(clampDepth(9)).toBe(4);
    expect(clampDepth(Number.NaN)).toBe(2);
  });
});

describe("buildKeywordMessages", () => {
  it("한국 종목: 영어 표기는 비우라고 하고, 종목과 깊이를 넘긴다", () => {
    const [system, user] = buildKeywordMessages(SAMSUNG, 2);
    // 부모마다 5개로 읽고 더 많이 만드는 일이 있어(2026-10-20 실측) 전체 개수를 분명히 적는다.
    expect(system.content).toContain("단계마다 모두 합쳐 정확히 5개");
    expect(system.content).toContain("부모 하나당 5개가 아니라");
    expect(system.content).toContain("keywordEn은 빈 문자열");
    expect(user.content).toBe(
      "종목: 삼성전자 (한국, KOSPI, 005930)\n깊이: 2 (단계마다 5개, 전체 10개)",
    );
  });

  it("미국 종목: 한글·영문 이름을 함께 주고 영어 표기를 요청한다", () => {
    const [system, user] = buildKeywordMessages(NVIDIA, 3);
    expect(system.content).toContain("keywordEn에는 영어 뉴스 검색에 쓸");
    expect(user.content).toBe(
      "종목: 엔비디아 / NVIDIA Corporation (미국, NASDAQ, NVDA)\n깊이: 3 (단계마다 5개, 전체 15개)",
    );
  });

  it("출력 스키마는 부모를 이름으로 가리키는 평평한 목록", () => {
    expect(keywordTreeSchema.safeParse(sampleKeywordTree()).success).toBe(true);
  });
});

describe("buildKeywordTree", () => {
  it("깊이 2: 단계마다 5개, 경로는 종목 이름부터 이어 붙인다", () => {
    const tree = buildKeywordTree(SAMSUNG, sampleKeywordTree().keywords, 2);
    expect(tree).toHaveLength(10);
    expect(tree.filter((node) => node.level === 1)).toHaveLength(5);

    const hbm = tree.find((node) => node.textKo === "HBM")!;
    expect(hbm).toMatchObject({
      level: 1,
      parentKey: null,
      path: "삼성전자 → HBM",
      reason: "AI 서버용 고대역폭 메모리",
    });
    expect(tree.find((node) => node.textKo === "엔비디아")).toMatchObject({
      level: 2,
      parentKey: hbm.key,
      path: "삼성전자 → HBM → 엔비디아",
      reason: "HBM 주요 고객",
    });
  });

  it("한국 종목은 영어 표기를 버리고, 미국 종목은 남긴다(D-14)", () => {
    const keywords = sampleKeywordTree({ english: true }).keywords;
    expect(
      buildKeywordTree(SAMSUNG, keywords, 2).every((n) => n.textEn === null),
    ).toBe(true);

    const us = buildKeywordTree(NVIDIA, keywords, 2);
    expect(us.find((node) => node.textKo === "HBM")).toMatchObject({
      textEn: "HBM",
      path: "엔비디아 → HBM",
    });
    expect(us.find((node) => node.textKo === "D램 가격")?.textEn).toBe(
      "DRAM prices",
    );
  });

  it("미국 종목이라도 영어 표기가 비어 있으면 null", () => {
    const [node] = buildKeywordTree(NVIDIA, [raw(1, "데이터센터")], 1);
    expect(node.textEn).toBeNull();
  });

  it("요청한 깊이보다 깊은 단계는 버린다", () => {
    const tree = buildKeywordTree(SAMSUNG, sampleKeywordTree().keywords, 1);
    expect(tree.map((node) => node.level)).toEqual([1, 1, 1, 1, 1]);
  });

  it("단계마다 5개까지만 받는다(D-13)", () => {
    const keywords = ["가", "나", "다", "라", "마", "바", "사"].map((text) =>
      raw(1, text),
    );
    expect(
      buildKeywordTree(SAMSUNG, keywords, 1).map((node) => node.textKo),
    ).toEqual(["가", "나", "다", "라", "마"]);
  });

  it("부모가 바로 윗단계에 없으면 버린다", () => {
    const tree = buildKeywordTree(
      SAMSUNG,
      [
        raw(1, "메모리"),
        raw(2, "엔비디아", "메모리"),
        raw(2, "애플", "없는 키워드"),
        raw(2, "구글", ""),
        // 3단계의 부모는 2단계여야 한다(1단계 '메모리'는 안 됨).
        raw(3, "AI", "메모리"),
        raw(3, "데이터센터", "엔비디아"),
      ],
      3,
    );
    expect(tree.map((node) => node.path)).toEqual([
      "삼성전자 → 메모리",
      "삼성전자 → 메모리 → 엔비디아",
      "삼성전자 → 메모리 → 엔비디아 → 데이터센터",
    ]);
  });

  it("같은 키워드(공백·대소문자 무시)와 종목 이름·코드는 버린다", () => {
    const tree = buildKeywordTree(
      SAMSUNG,
      [
        raw(1, "HBM"),
        raw(1, "h b m"),
        raw(1, "삼성 전자"),
        raw(1, "005930"),
        raw(1, "  "),
        raw(2, "hbm", "HBM"),
        raw(2, "엔비디아", "hbm"),
      ],
      2,
    );
    expect(tree.map((node) => node.path)).toEqual([
      "삼성전자 → HBM",
      "삼성전자 → HBM → 엔비디아",
    ]);
  });

  it("공백을 정리하고 너무 긴 키워드(40자)·이유(80자)는 자른다", () => {
    const [first, second] = buildKeywordTree(
      SAMSUNG,
      [
        raw(1, "  고대역폭\n  메모리  ", "", { reason: "가".repeat(100) }),
        raw(1, "나".repeat(50)),
      ],
      1,
    );
    expect(first.textKo).toBe("고대역폭 메모리");
    expect(first.reason).toHaveLength(80);
    expect(second.textKo).toHaveLength(40);
  });

  it("어떤 단계가 비면 그 아래 단계는 이어 붙일 곳이 없어 멈춘다", () => {
    const tree = buildKeywordTree(
      SAMSUNG,
      [raw(1, "메모리"), raw(3, "AI", "엔비디아")],
      3,
    );
    expect(tree.map((node) => node.textKo)).toEqual(["메모리"]);
  });

  it("LLM이 단계 순서를 섞어 줘도 단계 순서로 만든다", () => {
    const tree = buildKeywordTree(
      SAMSUNG,
      [raw(2, "엔비디아", "HBM"), raw(1, "HBM")],
      2,
    );
    expect(tree.map((node) => [node.key, node.parentKey, node.textKo])).toEqual(
      [
        [1, null, "HBM"],
        [2, 1, "엔비디아"],
      ],
    );
  });
});
