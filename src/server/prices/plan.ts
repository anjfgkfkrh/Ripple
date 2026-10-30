import type { Market } from "@/server/db/schema";

/**
 * 주가를 받는 시간(13단계, 2026-10-30 결정). 휴장일 달력은 두지 않는다(평일이면 받는다).
 * - 한국: 한국 시간 평일 08:00~20:00(통합 시세, 넥스트레이드 프리·애프터마켓 포함)
 * - 미국: 뉴욕 시간 평일 09:30~16:00(정규장, 일광절약시간은 시간대 계산이 맡는다)
 * 끝난 뒤 몇 분 더 받아 마지막 체결가를 남긴다.
 */
const WINDOWS: Record<
  Market,
  { timeZone: string; open: number; close: number }
> = {
  KR: { timeZone: "Asia/Seoul", open: 8 * 60, close: 20 * 60 },
  US: { timeZone: "America/New_York", open: 9 * 60 + 30, close: 16 * 60 },
};
/** 장이 끝난 뒤 더 받는 시간(분) */
export const PRICE_CLOSE_GRACE_MINUTES = 5;

const formatters = new Map<string, Intl.DateTimeFormat>();

function localClock(
  date: Date,
  timeZone: string,
): { weekday: string; minutes: number } {
  let formatter = formatters.get(timeZone);
  if (!formatter) {
    formatter = new Intl.DateTimeFormat("en-US", {
      timeZone,
      weekday: "short",
      hour: "2-digit",
      minute: "2-digit",
      hourCycle: "h23",
    });
    formatters.set(timeZone, formatter);
  }
  const parts: Record<string, string> = {};
  for (const { type, value } of formatter.formatToParts(date)) {
    parts[type] = value;
  }
  return {
    weekday: parts.weekday,
    minutes: Number(parts.hour) * 60 + Number(parts.minute),
  };
}

/** 지금 장이 열려 있는지(그 시장 시간 평일, 여는 시각 ≤ 지금 < 닫는 시각) */
export function isMarketOpen(market: Market, now: Date): boolean {
  return inWindow(market, now, 0);
}

/** 지금 주가를 받을 때인지(장중과 끝난 뒤 몇 분) */
export function isPriceWindow(market: Market, now: Date): boolean {
  return inWindow(market, now, PRICE_CLOSE_GRACE_MINUTES);
}

function inWindow(market: Market, now: Date, grace: number): boolean {
  const window = WINDOWS[market];
  const { weekday, minutes } = localClock(now, window.timeZone);
  if (weekday === "Sat" || weekday === "Sun") return false;
  return minutes >= window.open && minutes < window.close + grace;
}
