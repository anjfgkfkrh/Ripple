import { describe, expect, it } from "vitest";
import { effectivePublishedAt, FUTURE_TOLERANCE_MS } from "./published";

const SEEN = new Date("2026-10-22T16:29:00Z");
const at = (ms: number) => new Date(SEEN.getTime() + ms);

describe("effectivePublishedAt (2026-10-23 결정)", () => {
  it("없거나 받은 시각보다 10분 넘게 미래면 받은 시각, 아니면 피드 시각", () => {
    expect(effectivePublishedAt(null, SEEN)).toEqual(SEEN);
    // 국민일보 지면 기사: 19시간 미래
    expect(effectivePublishedAt(at(19 * 3600_000), SEEN)).toEqual(SEEN);
    expect(effectivePublishedAt(at(FUTURE_TOLERANCE_MS + 1), SEEN)).toEqual(
      SEEN,
    );
    // 몇 분 정도는 시계 차이로 보고 그대로 둔다.
    expect(effectivePublishedAt(at(FUTURE_TOLERANCE_MS), SEEN)).toEqual(
      at(FUTURE_TOLERANCE_MS),
    );
    expect(effectivePublishedAt(at(-3600_000), SEEN)).toEqual(at(-3600_000));
  });
});
