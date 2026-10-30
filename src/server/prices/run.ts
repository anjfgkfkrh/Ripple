import type { FinnhubQuoteClient } from "@/server/collect/finnhub";
import type { Db } from "@/server/db/client";
import type { Market, PriceSource } from "@/server/db/schema";
import { log, logError } from "@/server/log";
import { writeSetting } from "@/server/settings";
import { listActiveStocks } from "@/server/stocks/repository";
import type { KisClient } from "./kis";
import { isPriceWindow } from "./plan";
import { latestPrice, savePrice } from "./store";

export type PriceRefresher = {
  /** 받을 때가 된 종목의 주가를 받아 쌓는다. 던지지 않는다. 받으려 한 종목이 없으면 false */
  refresh(): Promise<boolean>;
};

/**
 * 종목 주가 받기(13단계). 스케줄러가 1분마다 부른다.
 * 장중(plan.ts)이거나 아직 한 번도 받지 못한 종목만 받는다. 한국 종목은 KIS, 미국 종목은 Finnhub(2026-10-30 결정).
 * 한 종목이 실패해도 나머지는 받고, 오류는 상태 화면에 보이게 남긴다.
 */
export function createPriceRefresher(deps: {
  db: Db;
  kis: KisClient | null;
  finnhub: FinnhubQuoteClient | null;
  now?: () => Date;
}): PriceRefresher {
  const now = deps.now ?? (() => new Date());
  let lastError = "";

  return {
    async refresh() {
      const at = now();
      const stocks = listActiveStocks(deps.db);
      const missing = new Set<string>();
      const errors: string[] = [];
      let attempted = 0;
      for (const stock of stocks) {
        if (stock.market === "KR" && !deps.kis) {
          missing.add("KIS 키(KIS_APP_KEY·KIS_APP_SECRET)가 없어 한국 종목");
          continue;
        }
        if (stock.market === "US" && !deps.finnhub) {
          missing.add("FINNHUB_API_KEY가 없어 미국 종목");
          continue;
        }
        if (
          !isPriceWindow(stock.market, at) &&
          latestPrice(deps.db, stock.id) !== null
        ) {
          continue;
        }
        attempted++;
        try {
          const quote = await fetchQuote(stock.market, stock.code, deps);
          if (!quote) throw new Error("값이 없습니다.");
          savePrice(deps.db, {
            stockId: stock.id,
            price: quote.price,
            change: quote.change,
            changePercent: quote.changePercent,
            source: quote.source,
            quotedAt: quote.quotedAt,
            fetchedAt: now(),
          });
        } catch (error) {
          errors.push(
            `${stock.nameKo ?? stock.name}: ${error instanceof Error ? error.message : String(error)}`,
          );
        }
      }
      try {
        writeSetting(
          deps.db,
          "prices.note",
          missing.size > 0
            ? `${[...missing].join(", ")} 주가는 받지 않습니다.`
            : "",
        );
        if (attempted === 0) return false;
        const error = errors.join(" / ");
        writeSetting(deps.db, "prices.checkedAt", at.getTime());
        writeSetting(deps.db, "prices.error", error);
        // 1분마다 같은 오류를 되풀이해 남기지 않게 바뀔 때만 로그에 쓴다.
        if (error !== lastError) {
          if (error) log(`주가 받기 실패: ${error}`);
          else if (lastError) log("주가 받기 다시 정상");
          lastError = error;
        }
      } catch (error) {
        logError("주가 상태 저장 실패", error);
      }
      return attempted > 0;
    },
  };
}

async function fetchQuote(
  market: Market,
  code: string,
  deps: { kis: KisClient | null; finnhub: FinnhubQuoteClient | null },
): Promise<{
  price: number;
  change: number;
  changePercent: number;
  source: PriceSource;
  quotedAt: Date | null;
} | null> {
  if (market === "KR") {
    if (!deps.kis) return null;
    // KIS 현재가 응답에는 시세 시각이 없어 받은 시각을 쓴다.
    return {
      ...(await deps.kis.domesticQuote(code)),
      source: "kis",
      quotedAt: null,
    };
  }
  if (!deps.finnhub) return null;
  const quote = await deps.finnhub.quote(code);
  return quote && { ...quote, source: "finnhub" };
}
