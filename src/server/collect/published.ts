/**
 * 피드가 준 발행 시각이 처음 받은 시각보다 이만큼 넘게 미래면 믿지 않는다(2026-10-23 결정).
 * 국민일보 피드는 다음 날 지면 기사에 4~24시간 미래 시각을 붙여 보낸다. 몇 분 정도는 시계 차이로 본다.
 */
export const FUTURE_TOLERANCE_MS = 10 * 60 * 1000;

/** 저장할 발행 시각: 없거나 미래면 처음 받은 시각 */
export function effectivePublishedAt(
  publishedAt: Date | null,
  seenAt: Date,
): Date {
  if (!publishedAt) return seenAt;
  return publishedAt.getTime() > seenAt.getTime() + FUTURE_TOLERANCE_MS
    ? seenAt
    : publishedAt;
}
