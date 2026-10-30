import { describe, expect, it } from "vitest";
import { isMarketOpen, isPriceWindow } from "./plan";

/** 한국 시간 "YYYY-MM-DD HH:MM"을 Date로 */
const kst = (text: string) => new Date(`${text.replace(" ", "T")}:00+09:00`);

describe("주가 받는 시간 (13단계)", () => {
  it("한국: 평일 08:00~20:00이 장중이고, 끝난 뒤 5분까지 받는다", () => {
    // 2026-10-30은 금요일
    expect(isMarketOpen("KR", kst("2026-10-30 07:59"))).toBe(false);
    expect(isMarketOpen("KR", kst("2026-10-30 08:00"))).toBe(true);
    expect(isMarketOpen("KR", kst("2026-10-30 19:59"))).toBe(true);
    expect(isMarketOpen("KR", kst("2026-10-30 20:00"))).toBe(false);
    expect(isPriceWindow("KR", kst("2026-10-30 20:04"))).toBe(true);
    expect(isPriceWindow("KR", kst("2026-10-30 20:05"))).toBe(false);
    // 토·일요일
    expect(isPriceWindow("KR", kst("2026-10-31 10:00"))).toBe(false);
    expect(isPriceWindow("KR", kst("2026-11-01 10:00"))).toBe(false);
  });

  it("미국: 뉴욕 평일 09:30~16:00. 일광절약시간이 끝나면 한국 시간으로 한 시간 늦어진다", () => {
    // 2026-10-30(금) 뉴욕은 일광절약시간(UTC-4): 한국 22:30에 열고 다음 날 05:00에 닫는다.
    expect(isMarketOpen("US", kst("2026-10-30 22:29"))).toBe(false);
    expect(isMarketOpen("US", kst("2026-10-30 22:30"))).toBe(true);
    expect(isMarketOpen("US", kst("2026-10-31 04:59"))).toBe(true);
    expect(isMarketOpen("US", kst("2026-10-31 05:00"))).toBe(false);
    expect(isPriceWindow("US", kst("2026-10-31 05:04"))).toBe(true);
    // 2026-11-02(월)은 표준시(UTC-5): 한국 23:30에 연다.
    expect(isMarketOpen("US", kst("2026-11-02 22:30"))).toBe(false);
    expect(isMarketOpen("US", kst("2026-11-02 23:30"))).toBe(true);
    // 한국 토요일 오전은 뉴욕 금요일 장중, 한국 월요일 오전은 뉴욕 일요일
    expect(isMarketOpen("US", kst("2026-11-02 02:00"))).toBe(false);
  });
});
