import { count, eq } from "drizzle-orm";
import type { Db } from "@/server/db/client";
import { listings, type Market } from "@/server/db/schema";
import { readSetting, writeSetting } from "@/server/settings";
import {
  parseKindList,
  parseNasdaqListed,
  parseOtherListed,
  type ListingRow,
} from "./parse";

/** 공식 종목 목록 원천 (D-12). 둘 다 무료이고 키가 필요 없다. */
export const LISTING_SOURCES = {
  kind: "https://kind.krx.co.kr/corpgeneral/corpList.do?method=download&searchType=13",
  nasdaqListed: "https://www.nasdaqtrader.com/dynamic/SymDir/nasdaqlisted.txt",
  otherListed: "https://www.nasdaqtrader.com/dynamic/SymDir/otherlisted.txt",
};
export type ListingSources = typeof LISTING_SOURCES;

/** 주 1회 갱신 (D-12) */
export const LISTINGS_MAX_AGE_MS = 7 * 24 * 60 * 60 * 1000;

const USER_AGENT = "Ripple/0.1 (personal stock news reader)";

type SyncOptions = {
  sources?: ListingSources;
  fetchImpl?: typeof fetch;
  now?: () => Date;
};

export type MarketSyncResult =
  | { market: Market; ok: true; count: number }
  | { market: Market; ok: false; error: string };

const SYNC_KEY = {
  KR: "listings.syncedAt.KR",
  US: "listings.syncedAt.US",
} as const;

/** 이 시장의 목록이 없거나 오래됐으면 받는다. 시장마다 따로 성공·실패한다. */
export async function syncListingsIfStale(
  db: Db,
  options: SyncOptions = {},
): Promise<MarketSyncResult[]> {
  const now = (options.now ?? (() => new Date()))().getTime();
  const stale = (["KR", "US"] as const).filter(
    (market) => now - readSetting(db, SYNC_KEY[market]) >= LISTINGS_MAX_AGE_MS,
  );
  const results: MarketSyncResult[] = [];
  for (const market of stale) {
    results.push(await syncMarket(db, market, options));
  }
  return results;
}

export async function syncMarket(
  db: Db,
  market: Market,
  options: SyncOptions = {},
): Promise<MarketSyncResult> {
  const sources = options.sources ?? LISTING_SOURCES;
  const fetchImpl = options.fetchImpl ?? fetch;
  try {
    const rows =
      market === "KR"
        ? parseKindList(
            new TextDecoder("euc-kr").decode(
              await download(fetchImpl, sources.kind),
            ),
          )
        : [
            ...parseNasdaqListed(
              new TextDecoder().decode(
                await download(fetchImpl, sources.nasdaqListed),
              ),
            ),
            ...parseOtherListed(
              new TextDecoder().decode(
                await download(fetchImpl, sources.otherListed),
              ),
            ),
          ];
    replaceListings(db, market, rows);
    writeSetting(
      db,
      SYNC_KEY[market],
      (options.now ?? (() => new Date()))().getTime(),
    );
    return { market, ok: true, count: rows.length };
  } catch (error) {
    return {
      market,
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

/**
 * 시장 단위로 통째로 갈아 끼운다(한 트랜잭션이라 읽는 쪽은 이전 목록 또는 새 목록만 본다).
 * 형식이 바뀌어 일부만 읽혔을 때 목록을 망가뜨리지 않도록, 이전의 절반보다 적으면 거부한다.
 */
export function replaceListings(
  db: Db,
  market: Market,
  rows: ListingRow[],
): void {
  const previous = listingCount(db, market);
  if (rows.length === 0 || rows.length < previous / 2) {
    throw new Error(
      `${market} 종목 목록이 비정상적으로 적습니다(새 ${rows.length}개, 이전 ${previous}개).`,
    );
  }
  const unique = new Map(rows.map((row) => [row.code, row]));
  db.transaction((tx) => {
    tx.delete(listings).where(eq(listings.market, market)).run();
    const values = [...unique.values()];
    for (let i = 0; i < values.length; i += 500) {
      tx.insert(listings)
        .values(values.slice(i, i + 500))
        .run();
    }
  });
}

export function listingCount(db: Db, market: Market): number {
  return (
    db
      .select({ n: count() })
      .from(listings)
      .where(eq(listings.market, market))
      .get()?.n ?? 0
  );
}

async function download(
  fetchImpl: typeof fetch,
  url: string,
): Promise<ArrayBuffer> {
  const response = await fetchImpl(url, {
    headers: { "User-Agent": USER_AGENT },
    signal: AbortSignal.timeout(60_000),
  });
  if (!response.ok) throw new Error(`${url} 응답 ${response.status}`);
  return response.arrayBuffer();
}
