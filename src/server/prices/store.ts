import { desc, eq } from "drizzle-orm";
import type { Db } from "@/server/db/client";
import { stockPrices, type Market } from "@/server/db/schema";
import { listActiveStocks } from "@/server/stocks/repository";
import { isMarketOpen } from "./plan";

export type StockPrice = typeof stockPrices.$inferSelect;

/** 받은 주가 한 줄을 쌓는다(worker만, 13단계: 덮어쓰지 않고 이력으로). */
export function savePrice(
  db: Db,
  row: Omit<typeof stockPrices.$inferInsert, "id">,
): void {
  db.insert(stockPrices).values(row).run();
}

/** 종목의 가장 최근 주가. 받은 적이 없으면 null */
export function latestPrice(db: Db, stockId: number): StockPrice | null {
  return (
    db
      .select()
      .from(stockPrices)
      .where(eq(stockPrices.stockId, stockId))
      .orderBy(desc(stockPrices.fetchedAt), desc(stockPrices.id))
      .limit(1)
      .get() ?? null
  );
}

export type PriceStripItem = {
  stockId: number;
  name: string;
  market: Market;
  price: number;
  change: number;
  changePercent: number;
  /** 시세 시각(받은 곳이 알려 주지 않으면 받은 시각) */
  at: Date;
  /** 지금 그 시장이 열려 있는지. 닫혀 있으면 화면에 "장 마감" */
  open: boolean;
};

/** 타임라인 맨 위 주가 띠(13단계). 등록 순서대로, 주가를 받은 적 있는 종목만 */
export function readPriceStrip(db: Db, now: Date): PriceStripItem[] {
  return listActiveStocks(db).flatMap((stock) => {
    const price = latestPrice(db, stock.id);
    if (!price) return [];
    return [
      {
        stockId: stock.id,
        name: stock.nameKo ?? stock.name,
        market: stock.market,
        price: price.price,
        change: price.change,
        changePercent: price.changePercent,
        at: price.quotedAt ?? price.fetchedAt,
        open: isMarketOpen(stock.market, now),
      },
    ];
  });
}
