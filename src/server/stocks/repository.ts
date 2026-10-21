import { and, asc, eq, isNull } from "drizzle-orm";
import type { Db } from "@/server/db/client";
import { stocks, type Market } from "@/server/db/schema";
import { findListing } from "@/server/listings/lookup";

export type Stock = typeof stocks.$inferSelect;

export const MAX_NAME_KO_LENGTH = 50;

/** 삭제하지 않은 종목, 등록한 순서대로 */
export function listActiveStocks(db: Db): Stock[] {
  return db
    .select()
    .from(stocks)
    .where(isNull(stocks.deletedAt))
    .orderBy(asc(stocks.createdAt), asc(stocks.id))
    .all();
}

export function isActiveStock(db: Db, market: Market, code: string): boolean {
  return Boolean(
    db
      .select({ id: stocks.id })
      .from(stocks)
      .where(
        and(
          eq(stocks.market, market),
          eq(stocks.code, code),
          isNull(stocks.deletedAt),
        ),
      )
      .get(),
  );
}

export function isActiveStockId(db: Db, id: number): boolean {
  return Boolean(
    db
      .select({ id: stocks.id })
      .from(stocks)
      .where(and(eq(stocks.id, id), isNull(stocks.deletedAt)))
      .get(),
  );
}

export type RegisterInput = {
  market: Market;
  code: string;
  userInput: string;
  nameKo?: string | null;
};

export type RegisterResult =
  | { status: "created" | "restored" | "exists"; stock: Stock }
  | { status: "not_listed" };

/**
 * 공식 목록에 있는 종목만 등록한다(D-12). 이름·거래소는 목록 값을 쓴다.
 * 예전에 지운 종목이면 새로 만들지 않고 되살린다(D-11).
 */
/** 새로 등록하거나 되살린 종목은 다음 수집 회차에서 이만큼 지난 기사까지 연결한다(2026-10-21 결정). */
export const NEW_STOCK_BACKFILL_MS = 24 * 60 * 60 * 1000;

export function registerStock(
  db: Db,
  input: RegisterInput,
  now: Date = new Date(),
): RegisterResult {
  const listing = findListing(db, input.market, input.code);
  if (!listing) return { status: "not_listed" };

  const nameKo =
    listing.market === "KR"
      ? listing.name
      : input.nameKo?.trim().slice(0, MAX_NAME_KO_LENGTH) || null;
  const fields = {
    name: listing.name,
    nameKo,
    exchange: listing.exchange,
    userInput: input.userInput.trim(),
    backfillFrom: new Date(now.getTime() - NEW_STOCK_BACKFILL_MS),
  };

  const existing = db
    .select()
    .from(stocks)
    .where(
      and(eq(stocks.market, listing.market), eq(stocks.code, listing.code)),
    )
    .get();
  if (existing && !existing.deletedAt) {
    return { status: "exists", stock: existing };
  }
  if (existing) {
    const stock = db
      .update(stocks)
      .set({ ...fields, deletedAt: null })
      .where(eq(stocks.id, existing.id))
      .returning()
      .get();
    return { status: "restored", stock };
  }
  const stock = db
    .insert(stocks)
    .values({
      market: listing.market,
      code: listing.code,
      ...fields,
      createdAt: now,
    })
    .returning()
    .get();
  return { status: "created", stock };
}

/** 지우지 않고 숨긴다(D-11). 이미 숨겼거나 없으면 false */
export function deleteStock(
  db: Db,
  id: number,
  now: Date = new Date(),
): boolean {
  const result = db
    .update(stocks)
    .set({ deletedAt: now })
    .where(and(eq(stocks.id, id), isNull(stocks.deletedAt)))
    .run();
  return result.changes > 0;
}
