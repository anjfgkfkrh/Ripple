import { describe, expect, it } from "vitest";
import {
  formatSeoulDateTime,
  formatTimelineTime,
  lastSeoulBoundary,
  nextSeoulBoundary,
  seoulDateKey,
  startOfSeoulMonth,
  timelineDayLabel,
} from "./time";

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

describe("lastSeoulBoundary / nextSeoulBoundary (키워드 재생성 06:00)", () => {
  // 2026-10-21은 수요일, 2026-10-19는 월요일
  const at = (iso: string) => new Date(iso);

  it("매일: 한국 06:00 전이면 전날 06:00, 06:00부터는 그날 06:00", () => {
    // 한국 10-21 05:59
    expect(lastSeoulBoundary(at("2026-10-20T20:59:00Z"), 6).toISOString()).toBe(
      "2026-10-19T21:00:00.000Z",
    );
    // 한국 10-21 06:00 정각
    expect(lastSeoulBoundary(at("2026-10-20T21:00:00Z"), 6).toISOString()).toBe(
      "2026-10-20T21:00:00.000Z",
    );
    // 한국 10-21 23:30
    expect(lastSeoulBoundary(at("2026-10-21T14:30:00Z"), 6).toISOString()).toBe(
      "2026-10-20T21:00:00.000Z",
    );
    expect(nextSeoulBoundary(at("2026-10-21T14:30:00Z"), 6).toISOString()).toBe(
      "2026-10-21T21:00:00.000Z",
    );
  });

  it("일주일: 가장 최근 월요일 06:00", () => {
    // 한국 10-21(수) 12:00 → 10-19(월) 06:00
    expect(
      lastSeoulBoundary(at("2026-10-21T03:00:00Z"), 6, true).toISOString(),
    ).toBe("2026-10-18T21:00:00.000Z");
    // 한국 10-19(월) 05:00 → 그 전 주 월요일 10-12 06:00
    expect(
      lastSeoulBoundary(at("2026-10-18T20:00:00Z"), 6, true).toISOString(),
    ).toBe("2026-10-11T21:00:00.000Z");
    // 한국 10-19(월) 06:00 정각 → 그날
    expect(
      lastSeoulBoundary(at("2026-10-18T21:00:00Z"), 6, true).toISOString(),
    ).toBe("2026-10-18T21:00:00.000Z");
    // 한국 10-25(일) 23:00 → 다음은 10-26(월) 06:00
    expect(
      nextSeoulBoundary(at("2026-10-25T14:00:00Z"), 6, true).toISOString(),
    ).toBe("2026-10-25T21:00:00.000Z");
  });
});

describe("타임라인 시각 (2026-10-21 결정)", () => {
  // 한국 시간 2026-10-21(수) 00:30
  const now = new Date("2026-10-20T15:30:00Z");

  it("오늘·어제는 '오늘 14:05'처럼, 그 전은 '10-19 08:00', 해가 다르면 연도까지", () => {
    // 한국 10-21 00:10
    expect(formatTimelineTime(new Date("2026-10-20T15:10:00Z"), now)).toBe(
      "오늘 00:10",
    );
    // 한국 10-20 23:59
    expect(formatTimelineTime(new Date("2026-10-20T14:59:00Z"), now)).toBe(
      "어제 23:59",
    );
    // 한국 10-19 08:00
    expect(formatTimelineTime(new Date("2026-10-18T23:00:00Z"), now)).toBe(
      "10-19 08:00",
    );
    expect(
      formatTimelineTime(
        new Date("2025-12-31T00:00:00Z"),
        new Date("2026-01-02T00:00:00Z"),
      ),
    ).toBe("2025-12-31 09:00");
  });

  it("날짜 구분선은 오늘·어제를 붙이고 요일을 쓴다", () => {
    expect(timelineDayLabel(new Date("2026-10-20T15:10:00Z"), now)).toBe(
      "오늘 · 10월 21일(수)",
    );
    expect(timelineDayLabel(new Date("2026-10-20T14:59:00Z"), now)).toBe(
      "어제 · 10월 20일(화)",
    );
    expect(timelineDayLabel(new Date("2026-10-18T23:00:00Z"), now)).toBe(
      "10월 19일(월)",
    );
    expect(
      timelineDayLabel(
        new Date("2025-12-31T00:00:00Z"),
        new Date("2026-01-02T00:00:00Z"),
      ),
    ).toBe("2025년 12월 31일(수)");
    expect(seoulDateKey(new Date("2026-10-20T15:10:00Z"))).toBe("2026-10-21");
  });
});
