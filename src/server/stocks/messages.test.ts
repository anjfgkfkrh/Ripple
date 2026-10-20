import { describe, expect, it } from "vitest";
import { describeStockError } from "./messages";

describe("describeStockError", () => {
  it("예산 초과면 한도를 함께 알려 준다", () => {
    expect(describeStockError("budget_exceeded", { limitUsd: 5 })).toBe(
      "이번 달 AI 예산 $5을 모두 써서 지금은 찾을 수 없습니다.",
    );
  });

  it("키가 없으면 어디를 고쳐야 하는지 알려 준다", () => {
    expect(describeStockError("llm_no_api_key")).toContain(
      "OPENROUTER_API_KEY",
    );
  });

  it("모르는 코드도 문장으로 바꾼다", () => {
    expect(describeStockError("something_new")).toBe(
      "처리하지 못했습니다. 다시 시도하세요.",
    );
  });
});
