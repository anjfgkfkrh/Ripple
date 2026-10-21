import { describe, expect, it } from "vitest";
import type { SourceResult } from "../db/schema";
import { groupFailures } from "./read";

const source = (label: string, error?: string): SourceResult => ({
  id: label,
  label,
  group: label,
  fetched: 0,
  fresh: 0,
  saved: 0,
  matched: 0,
  ...(error ? { error } : {}),
  ms: 1,
});

describe("groupFailures (2026-10-21 결정)", () => {
  it("실패한 소스를 같은 이유끼리 묶어 많은 것부터, 성공한 소스는 뺀다", () => {
    const robots = "robots.txt가 막고 있거나 확인하지 못해 건너뛰었습니다.";
    expect(
      groupFailures([
        source("가경제 경제", robots),
        source("나일보 전체"),
        source("Finnhub 시장 뉴스", "HTTP 500"),
        source("다일보 전체", robots),
        source("라일보 전체", robots),
      ]),
    ).toEqual([
      {
        reason: robots,
        labels: ["가경제 경제", "다일보 전체", "라일보 전체"],
      },
      { reason: "HTTP 500", labels: ["Finnhub 시장 뉴스"] },
    ]);
    expect(groupFailures([source("나일보 전체")])).toEqual([]);
  });
});
