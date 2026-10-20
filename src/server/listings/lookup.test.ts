import { describe, expect, it } from "vitest";
import { normalizeCode } from "./lookup";

describe("normalizeCode", () => {
  it.each([
    ["005930", "005930"],
    [" a005930 ", "005930"],
    ["5930", "005930"],
    ["0158s0", "0158S0"],
    ["005-930", "005930"],
  ])("한국 코드 %s → %s", (input, expected) => {
    expect(normalizeCode("KR", input)).toBe(expected);
  });

  it.each([
    ["nvda", "NVDA"],
    ["BRK-B", "BRK.B"],
    ["BRK/B", "BRK.B"],
    [" tsm ", "TSM"],
  ])("미국 티커 %s → %s", (input, expected) => {
    expect(normalizeCode("US", input)).toBe(expected);
  });
});
