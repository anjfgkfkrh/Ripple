import { and, eq } from "drizzle-orm";
import type { Db } from "@/server/db/client";
import { listings, type Market } from "@/server/db/schema";
import { KR_CODE } from "./parse";

export type Listing = typeof listings.$inferSelect;

/**
 * LLM이 준 코드를 목록 형식에 맞춘다.
 * - 한국: "A005930"처럼 앞에 붙는 A를 떼고, 숫자만 짧게 오면 앞을 0으로 채운다.
 * - 미국: 클래스 주식 구분자는 목록 형식인 점으로 맞춘다(BRK-B, BRK/B → BRK.B).
 */
export function normalizeCode(market: Market, code: string): string {
  const upper = code.trim().toUpperCase();
  if (market === "US") return upper.replace(/[-/ ]/g, ".");
  const compact = upper.replace(/[^0-9A-Z]/g, "");
  if (/^A[0-9A-Z]{6}$/.test(compact)) return compact.slice(1);
  if (/^\d{1,5}$/.test(compact)) return compact.padStart(6, "0");
  return compact;
}

export function findListing(
  db: Db,
  market: Market,
  code: string,
): Listing | undefined {
  const normalized = normalizeCode(market, code);
  if (market === "KR" && !KR_CODE.test(normalized)) return undefined;
  return db
    .select()
    .from(listings)
    .where(and(eq(listings.market, market), eq(listings.code, normalized)))
    .get();
}

export function findListingByName(
  db: Db,
  market: Market,
  name: string,
): Listing | undefined {
  return db
    .select()
    .from(listings)
    .where(and(eq(listings.market, market), eq(listings.name, name.trim())))
    .get();
}
