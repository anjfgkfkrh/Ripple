import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import { FIXTURE_FILES, fixtureBytes } from "../../../tests/fixtures";
import { openDb, type Db } from "../db/client";
import { readSetting } from "../settings";
import { findListing } from "./lookup";
import {
  LISTINGS_MAX_AGE_MS,
  listingCount,
  replaceListings,
  syncListingsIfStale,
  syncMarket,
  type ListingSources,
} from "./sync";

const SOURCES: ListingSources = {
  kind: "http://fixtures/kind",
  nasdaqListed: "http://fixtures/nasdaqlisted",
  otherListed: "http://fixtures/otherlisted",
};

/** 표본 파일을 돌려주는 가짜 fetch. 받은 요청을 남긴다. */
function fixtureFetch(failing: string[] = []) {
  const requests: { url: string; userAgent: string | null }[] = [];
  const files: Record<string, string> = {
    [SOURCES.kind]: FIXTURE_FILES.kind,
    [SOURCES.nasdaqListed]: FIXTURE_FILES.nasdaqListed,
    [SOURCES.otherListed]: FIXTURE_FILES.otherListed,
  };
  const fetchImpl = (async (
    input: string | URL | Request,
    init?: RequestInit,
  ) => {
    const url = String(input);
    requests.push({
      url,
      userAgent: new Headers(init?.headers).get("user-agent"),
    });
    if (failing.includes(url)) return new Response("down", { status: 503 });
    return new Response(new Uint8Array(fixtureBytes(files[url])));
  }) as typeof fetch;
  return { fetchImpl, requests };
}

const opened: Db[] = [];
const cleanups: (() => void)[] = [];
function testDb(): Db {
  const file = createTestDbFile();
  cleanups.push(file.cleanup);
  const db = openDb(file.path, "readwrite");
  opened.push(db);
  return db;
}

afterEach(() => {
  opened.splice(0).forEach((db) => db.$client.close());
  cleanups.splice(0).forEach((cleanup) => cleanup());
});

describe("공식 종목 목록 동기화 (SPEC D-12)", () => {
  it("한국은 KIND(EUC-KR), 미국은 NASDAQ 두 파일을 받아 저장하고 시각을 남긴다", async () => {
    const db = testDb();
    const { fetchImpl, requests } = fixtureFetch();
    const now = new Date("2026-10-20T00:00:00Z");

    const results = await syncListingsIfStale(db, {
      sources: SOURCES,
      fetchImpl,
      now: () => now,
    });

    expect(results).toEqual([
      { market: "KR", ok: true, count: 7 },
      { market: "US", ok: true, count: 9 },
    ]);
    expect(findListing(db, "KR", "005930")?.name).toBe("삼성전자");
    expect(findListing(db, "US", "NVDA")?.exchange).toBe("NASDAQ");
    expect(readSetting(db, "listings.syncedAt.KR")).toBe(now.getTime());
    expect(requests).toHaveLength(3);
    expect(requests.every((r) => r.userAgent?.startsWith("Ripple/"))).toBe(
      true,
    );
  });

  it("받은 지 7일이 안 됐으면 다시 받지 않는다", async () => {
    const db = testDb();
    const start = new Date("2026-10-20T00:00:00Z");
    await syncListingsIfStale(db, {
      sources: SOURCES,
      fetchImpl: fixtureFetch().fetchImpl,
      now: () => start,
    });

    const later = fixtureFetch();
    const sixDays = new Date(start.getTime() + LISTINGS_MAX_AGE_MS - 1);
    expect(
      await syncListingsIfStale(db, {
        sources: SOURCES,
        fetchImpl: later.fetchImpl,
        now: () => sixDays,
      }),
    ).toEqual([]);
    expect(later.requests).toHaveLength(0);
  });

  it("한 시장이 실패해도 다른 시장은 갱신하고, 실패한 시장은 이전 목록을 유지한다", async () => {
    const db = testDb();
    await syncMarket(db, "KR", {
      sources: SOURCES,
      fetchImpl: fixtureFetch().fetchImpl,
    });

    const results = await syncListingsIfStale(db, {
      sources: SOURCES,
      fetchImpl: fixtureFetch([SOURCES.kind]).fetchImpl,
      now: () => new Date(Date.now() + LISTINGS_MAX_AGE_MS),
    });

    expect(results[0]).toMatchObject({ market: "KR", ok: false });
    expect(results[1]).toMatchObject({ market: "US", ok: true });
    expect(listingCount(db, "KR")).toBe(7);
  });

  it("새 목록이 이전의 절반보다 적으면 형식이 바뀐 것으로 보고 거부한다", () => {
    const db = testDb();
    const row = (code: string) => ({
      market: "KR" as const,
      code,
      name: code,
      exchange: "KOSPI",
      isEtf: false,
    });
    replaceListings(
      db,
      "KR",
      ["000001", "000002", "000003", "000004"].map(row),
    );

    expect(() => replaceListings(db, "KR", [row("000005")])).toThrow(
      "비정상적으로 적습니다",
    );
    expect(() => replaceListings(db, "KR", [])).toThrow();
    expect(listingCount(db, "KR")).toBe(4);
  });
});
