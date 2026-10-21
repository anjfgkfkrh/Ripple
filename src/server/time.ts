/** 화면 표시와 스케줄 기준 시간대 (SPEC 4장) */
export const APP_TIME_ZONE = "Asia/Seoul";

const seoulFormatter = new Intl.DateTimeFormat("en-US", {
  timeZone: APP_TIME_ZONE,
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
  hourCycle: "h23",
});

/** 한국은 일광절약시간이 없어 항상 UTC+9다. */
const SEOUL_OFFSET_MS = 9 * 60 * 60 * 1000;

/** 이 시각이 속한 달(한국 시간 기준)의 1일 0시. 월 예산 합계의 기준이다. */
export function startOfSeoulMonth(date: Date): Date {
  const seoul = new Date(date.getTime() + SEOUL_OFFSET_MS);
  return new Date(
    Date.UTC(seoul.getUTCFullYear(), seoul.getUTCMonth(), 1) - SEOUL_OFFSET_MS,
  );
}

/** 한국 시간 "YYYY-MM-DD HH:mm:ss" */
export function formatSeoulDateTime(date: Date): string {
  const parts: Record<string, string> = {};
  for (const { type, value } of seoulFormatter.formatToParts(date)) {
    parts[type] = value;
  }
  return `${parts.year}-${parts.month}-${parts.day} ${parts.hour}:${parts.minute}:${parts.second}`;
}

const DAY_MS = 24 * 60 * 60 * 1000;

/**
 * now 이전(같은 시각 포함)의 가장 최근 한국 시간 hour시 정각. weekly면 그중 월요일.
 * 키워드 자동 재생성 시각(매일 06:00, 일주일 1회면 월요일 06:00)의 기준이다.
 */
export function lastSeoulBoundary(
  now: Date,
  hour: number,
  weekly = false,
): Date {
  const seoul = new Date(now.getTime() + SEOUL_OFFSET_MS);
  let boundary =
    Date.UTC(
      seoul.getUTCFullYear(),
      seoul.getUTCMonth(),
      seoul.getUTCDate(),
      hour,
    ) - SEOUL_OFFSET_MS;
  if (boundary > now.getTime()) boundary -= DAY_MS;
  if (weekly) {
    // 0=일요일 … 1=월요일. 한국 시간 날짜의 요일로 센다.
    const day = new Date(boundary + SEOUL_OFFSET_MS).getUTCDay();
    boundary -= ((day + 6) % 7) * DAY_MS;
  }
  return new Date(boundary);
}

/** now 다음(같은 시각 제외)의 한국 시간 hour시 정각. weekly면 그중 월요일. */
export function nextSeoulBoundary(
  now: Date,
  hour: number,
  weekly = false,
): Date {
  const last = lastSeoulBoundary(now, hour, weekly);
  return new Date(last.getTime() + (weekly ? 7 : 1) * DAY_MS);
}
