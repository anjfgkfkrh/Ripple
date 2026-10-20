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
