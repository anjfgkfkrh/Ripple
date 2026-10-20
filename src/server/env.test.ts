import { describe, expect, it } from "vitest";
import { readPort, requireEnv } from "./env";

describe("requireEnv", () => {
  it("값을 앞뒤 공백 없이 돌려준다", () => {
    expect(requireEnv("A", { A: "  value " })).toBe("value");
  });

  it("없거나 빈 값이면 이름을 담아 실패하고, 값은 메시지에 넣지 않는다", () => {
    expect(() => requireEnv("A", {})).toThrow("환경변수 A");
    expect(() => requireEnv("A", { A: "   " })).toThrow("환경변수 A");
  });
});

describe("readPort", () => {
  it("값이 없으면 기본값을 쓴다", () => {
    expect(readPort("P", 4000, {})).toBe(4000);
  });

  it("올바른 포트를 숫자로 읽는다", () => {
    expect(readPort("P", 4000, { P: "4100" })).toBe(4100);
  });

  it.each(["0", "65536", "abc", "40.5"])(
    "잘못된 값(%s)이면 실패한다",
    (value) => {
      expect(() => readPort("P", 4000, { P: value })).toThrow("환경변수 P");
    },
  );
});
