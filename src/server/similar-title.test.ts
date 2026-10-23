import { describe, expect, it } from "vitest";
import { isSameNews, titleFeatures } from "./similar-title";

const same = (a: string, b: string) =>
  isSameNews(titleFeatures(a), titleFeatures(b));

// 2026-10-22 실측에 쓴 실제 제목들
describe("isSameNews (11단계 비슷한 제목 기준)", () => {
  it("같은 소식을 언론사마다 다르게 쓴 제목은 같은 소식", () => {
    expect(
      same(
        "[속보] 한은, 기준금리 3.00% 동결",
        "한은, 기준금리 3.00% 동결…연속 인상 후 숨 고르기",
      ),
    ).toBe(true);
    expect(
      same(
        "美, 사우디 주재 공무원·가족 철수 명령… 후티 반군 공격 임박했나",
        "美, 사우디 주재 직원·가족 철수 명령…후티 위협에 경계 강화",
      ),
    ).toBe(true);
    expect(
      same(
        "‘매파적 동결’ 택한 한은…‘11월 인상’ 열어뒀다",
        "‘매파적 동결’ 택한 한은…‘11월 인상’ 문 열어둬",
      ),
    ).toBe(true);
    expect(
      same(
        "KT클라우드, 50MW 안산 AIDC 착공…수도권 AI 인프라 확대",
        "KT클라우드, 50MW 규모 안산 AI 데이터센터 착공",
      ),
    ).toBe(true);
  });

  it("틀만 같은 다른 회사 소식·우연히 겹친 영어 제목은 다른 소식", () => {
    expect(
      same(
        "LGD, 3분기 영업익 2506억원… 전년比 42%↓",
        "삼성E&A, 3분기 영업익 5006억원···전년比 183.6% 증가",
      ),
    ).toBe(false);
    expect(
      same(
        "Oil rises as US orders diplomats to leave Saudi Arabia",
        "Trump to rally for Graham in tight South Carolina race",
      ),
    ).toBe(false);
    expect(
      same("국제유가 혼조세 마감…브렌트유 108달러", "코스피 3.5% 급락 마감"),
    ).toBe(false);
  });

  it("머리말과 기호는 보지 않고, 빈 제목은 같은 소식이 아니다", () => {
    expect(same("[단독](종합) 금리 동결", "금리 동결")).toBe(true);
    expect(same("", "")).toBe(false);
    expect(same("…", "…")).toBe(false);
  });
});
