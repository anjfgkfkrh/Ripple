import { describe, expect, it } from "vitest";
import { formatSeoulDateTime, startOfSeoulMonth } from "./time";

describe("startOfSeoulMonth", () => {
  it("한국 시간 기준 그달 1일 0시를 돌려준다", () => {
    expect(
      startOfSeoulMonth(new Date("2026-10-20T03:00:00Z")).toISOString(),
    ).toBe("2026-09-30T15:00:00.000Z");
  });

  it("UTC로는 전달이어도 한국 시간으로 새달이면 새달 1일", () => {
    // 2026-10-31T16:00Z = 한국 2026-11-01 01:00
    expect(
      startOfSeoulMonth(new Date("2026-10-31T16:00:00Z")).toISOString(),
    ).toBe("2026-10-31T15:00:00.000Z");
    // 2026-10-31T14:59Z = 한국 2026-10-31 23:59 → 아직 10월
    expect(
      startOfSeoulMonth(new Date("2026-10-31T14:59:00Z")).toISOString(),
    ).toBe("2026-09-30T15:00:00.000Z");
  });

  it("연말도 넘어간다", () => {
    expect(
      startOfSeoulMonth(new Date("2026-12-31T15:30:00Z")).toISOString(),
    ).toBe("2026-12-31T15:00:00.000Z");
  });
});

describe("formatSeoulDateTime", () => {
  it("UTC 시각을 한국 시간(+9시간)으로 표시한다", () => {
    expect(formatSeoulDateTime(new Date("2026-10-19T00:00:00Z"))).toBe(
      "2026-10-19 09:00:00",
    );
  });

  it("날짜가 넘어가는 경우와 자정을 00시로 표시한다", () => {
    expect(formatSeoulDateTime(new Date("2026-12-31T15:00:00Z"))).toBe(
      "2027-01-01 00:00:00",
    );
    expect(formatSeoulDateTime(new Date("2026-03-05T14:59:59Z"))).toBe(
      "2026-03-05 23:59:59",
    );
  });
});
