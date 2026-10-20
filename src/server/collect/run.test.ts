import { asc, eq } from "drizzle-orm";
import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import { seedListings } from "../../../tests/fixtures";
import { openDb, type Db } from "../db/client";
import { articleMatches, articles, collectionRuns } from "../db/schema";
import { replaceKeywords } from "../keywords/generate";
import { deleteStock, registerStock } from "../stocks/repository";
import type { RssFeed } from "./feeds";
import type { FinnhubClient, FinnhubNews } from "./finnhub";
import type { HttpClient } from "./http";
import type { RobotsChecker } from "./robots";
import {
  closeStaleRuns,
  computeSince,
  startCollectionRun,
  SUMMARY_MAX_CHARS,
  type CollectDeps,
} from "./run";
import { createCollectionRunner } from "./runner";

const opened: Db[] = [];
const cleanups: (() => void)[] = [];
function testDb(): Db {
  const file = createTestDbFile();
  cleanups.push(file.cleanup);
  const db = openDb(file.path, "readwrite");
  opened.push(db);
  seedListings(db);
  return db;
}
afterEach(() => {
  opened.splice(0).forEach((db) => db.$client.close());
  cleanups.splice(0).forEach((cleanup) => cleanup());
});

// 한국 시간 2026-10-20 12:00
const NOW = new Date("2026-10-20T03:00:00Z");
const HOUR = 60 * 60 * 1000;
const ago = (hours: number) => new Date(NOW.getTime() - hours * HOUR);

function addStocks(db: Db) {
  const samsung = registerStock(db, {
    market: "KR",
    code: "005930",
    userInput: "삼전",
  });
  const nvidia = registerStock(db, {
    market: "US",
    code: "NVDA",
    userInput: "NVDA",
    nameKo: "엔비디아",
  });
  if (samsung.status === "not_listed" || nvidia.status === "not_listed")
    throw new Error("표본에 없음");
  replaceKeywords(
    db,
    samsung.stock.id,
    [
      {
        key: 1,
        parentKey: null,
        level: 1,
        textKo: "HBM 메모리",
        textEn: null,
        reason: "r",
        path: "삼성전자 → HBM 메모리",
      },
      {
        key: 2,
        parentKey: 1,
        level: 2,
        textKo: "엔비디아",
        textEn: null,
        reason: "r",
        path: "삼성전자 → HBM 메모리 → 엔비디아",
      },
    ],
    NOW,
  );
  replaceKeywords(
    db,
    nvidia.stock.id,
    [
      {
        key: 1,
        parentKey: null,
        level: 1,
        textKo: "AI 가속기",
        textEn: "AI accelerator",
        reason: "r",
        path: "엔비디아 → AI 가속기",
      },
    ],
    NOW,
  );
  return { samsung: samsung.stock.id, nvidia: nvidia.stock.id };
}

const item = (
  title: string,
  link: string,
  pubDate: Date | null,
  description = "",
) =>
  `<item><title>${title}</title><link>${link}</link><description>${description}</description>${
    pubDate ? `<pubDate>${pubDate.toUTCString()}</pubDate>` : ""
  }</item>`;
const rss = (...items: string[]) =>
  `<rss version="2.0"><channel>${items.join("")}</channel></rss>`;

function fakeHttp(
  routes: Record<string, { status?: number; text: string } | Error>,
) {
  const calls: string[] = [];
  const http: HttpClient = {
    async get(url) {
      calls.push(url);
      const r = routes[url];
      if (!r) return { status: 404, text: "", contentType: "" };
      if (r instanceof Error) throw r;
      return {
        status: r.status ?? 200,
        text: r.text,
        contentType: "application/rss+xml",
      };
    },
  };
  return { http, calls };
}

const allowAll: RobotsChecker = { allowed: async () => true };

const FEEDS: RssFeed[] = [
  {
    id: "rss:a:economy",
    publisher: "가경제",
    section: "경제",
    url: "https://a.example/economy.xml",
  },
  {
    id: "rss:b:all",
    publisher: "나일보",
    section: "전체",
    url: "https://b.example/all.xml",
  },
];

function deps(
  db: Db,
  routes: Parameters<typeof fakeHttp>[0],
  extra: Partial<CollectDeps> = {},
) {
  const { http, calls } = fakeHttp(routes);
  return {
    calls,
    deps: {
      db,
      http,
      robots: allowAll,
      feeds: FEEDS,
      finnhub: null,
      now: () => NOW,
      ...extra,
    } satisfies CollectDeps,
  };
}

const run = async (d: CollectDeps) => startCollectionRun(d, "manual").done;

describe("수집 회차 (SPEC FR-4)", () => {
  it("첫 회차: 최근 24시간 기사 중 종목에 걸린 것만 저장하고, 같은 주소는 한 기사로 합친다", async () => {
    const db = testDb();
    const ids = addStocks(db);
    const { deps: d } = deps(db, {
      "https://a.example/economy.xml": {
        text: rss(
          item(
            "삼성전자 3분기 실적",
            "https://news.example/1?utm_source=rss",
            ago(2),
          ),
          item("메모리 가격 오르자 HBM 증설", "https://news.example/2", ago(3)),
          item("날씨 소식", "https://news.example/3", ago(1)),
          item("삼성전자 오래된 기사", "https://news.example/old", ago(30)),
        ),
      },
      // 같은 기사(추적 파라미터만 다름)와 미국 종목 기사
      "https://b.example/all.xml": {
        text: rss(
          item("삼성전자 3분기 실적", "http://news.example/1/", ago(2)),
          item("엔비디아, 새 AI 가속기 공개", "https://news.example/4", ago(1)),
        ),
      },
    });

    const summary = await run(d);

    expect(summary).toMatchObject({
      status: "success",
      articlesNew: 3,
      matchesNew: 4,
    });
    const saved = db.select().from(articles).orderBy(asc(articles.id)).all();
    expect(saved.map((a) => [a.title, a.publisher, a.feed])).toEqual([
      ["삼성전자 3분기 실적", "가경제", "rss:a:economy"],
      ["메모리 가격 오르자 HBM 증설", "가경제", "rss:a:economy"],
      ["엔비디아, 새 AI 가속기 공개", "나일보", "rss:b:all"],
    ]);
    const links = db
      .select()
      .from(articleMatches)
      .orderBy(asc(articleMatches.id))
      .all();
    expect(
      links.map((m) => [
        m.articleId,
        m.stockId,
        m.keywordPath,
        m.matchKind,
        m.level,
      ]),
    ).toEqual([
      [saved[0].id, ids.samsung, "삼성전자", "name", 0],
      [saved[1].id, ids.samsung, "삼성전자 → HBM 메모리", "keyword", 1],
      // '엔비디아'는 삼성전자의 2단계 키워드이자 엔비디아 종목 이름
      [
        saved[2].id,
        ids.samsung,
        "삼성전자 → HBM 메모리 → 엔비디아",
        "keyword",
        2,
      ],
      [saved[2].id, ids.nvidia, "엔비디아", "name", 0],
    ]);

    const record = db.select().from(collectionRuns).get()!;
    expect(record).toMatchObject({
      trigger: "manual",
      status: "success",
      since: ago(24),
      finishedAt: NOW,
      articlesNew: 3,
      matchesNew: 4,
      note: null,
    });
    // 걸린 시간(ms)은 실행마다 달라 비교에서 뺀다.
    expect(record.sources.map((s) => ({ ...s, ms: undefined }))).toEqual([
      {
        id: "rss:a:economy",
        label: "가경제 경제",
        group: "가경제",
        fetched: 4,
        fresh: 3,
        saved: 2,
        matched: 2,
      },
      {
        id: "rss:b:all",
        label: "나일보 전체",
        group: "나일보",
        fetched: 2,
        fresh: 2,
        saved: 1,
        matched: 2,
      },
    ]);
  });

  it("한 소스가 실패해도 나머지는 계속하고 '일부 실패'로 남긴다. 모두 실패하면 '실패'", async () => {
    const db = testDb();
    addStocks(db);
    const partial = deps(db, {
      "https://a.example/economy.xml": { status: 500, text: "" },
      "https://b.example/all.xml": {
        text: rss(item("삼성전자 소식", "https://n.example/1", ago(1))),
      },
    });
    expect(await run(partial.deps)).toMatchObject({
      status: "partial",
      articlesNew: 1,
    });
    const record = db.select().from(collectionRuns).all()[0];
    expect(record.sources[0]).toMatchObject({
      id: "rss:a:economy",
      error: "HTTP 500",
      saved: 0,
    });

    const allFail = deps(db, {
      "https://a.example/economy.xml": new Error("연결 실패"),
      "https://b.example/all.xml": { status: 503, text: "" },
    });
    expect(await run(allFail.deps)).toMatchObject({ status: "failed" });
  });

  it("robots.txt가 막으면 받지 않고 그 이유를 남긴다", async () => {
    const db = testDb();
    addStocks(db);
    const blocked = deps(
      db,
      { "https://b.example/all.xml": { text: rss() } },
      {
        robots: {
          allowed: async (url) => !url.startsWith("https://a.example"),
        },
      },
    );
    await run(blocked.deps);
    expect(blocked.calls).toEqual(["https://b.example/all.xml"]);
    const record = db.select().from(collectionRuns).get()!;
    expect(record.sources[0].error).toContain("robots.txt");
  });

  it("등록한 종목이 없으면 아무것도 받지 않는다", async () => {
    const db = testDb();
    const { deps: d, calls } = deps(db, {});
    expect(await run(d)).toMatchObject({ status: "success", articlesNew: 0 });
    expect(calls).toEqual([]);
    expect(db.select().from(collectionRuns).get()?.note).toContain(
      "종목이 없어",
    );
  });

  it("삭제한 종목에는 걸지 않는다", async () => {
    const db = testDb();
    const ids = addStocks(db);
    deleteStock(db, ids.samsung);
    const { deps: d } = deps(db, {
      "https://a.example/economy.xml": {
        text: rss(item("삼성전자 소식", "https://n.example/1", ago(1))),
      },
    });
    expect(await run(d)).toMatchObject({ articlesNew: 0 });
  });

  it("발행 시각이 없는 기사는 처음 받은 시각으로 저장하고, 다시 받아도 한 번만 저장한다", async () => {
    const db = testDb();
    addStocks(db);
    const routes = {
      "https://a.example/economy.xml": {
        text: rss(item("삼성전자 단독", "https://n.example/x", null)),
      },
    };
    await run(deps(db, routes).deps);
    await run(
      deps(db, routes, { now: () => new Date(NOW.getTime() + HOUR) }).deps,
    );
    const saved = db.select().from(articles).all();
    expect(saved).toHaveLength(1);
    expect(saved[0].publishedAt).toEqual(NOW);
    expect(db.select().from(articleMatches).all()).toHaveLength(1);
  });

  it("요약 자리에 본문이 통째로 와도 앞 300자까지만 보고 건다", async () => {
    const db = testDb();
    addStocks(db);
    const { deps: d } = deps(db, {
      "https://a.example/economy.xml": {
        text: rss(
          item(
            "철강 생산 소식",
            "https://n.example/steel",
            ago(1),
            `${"가".repeat(400)} 삼성전자`,
          ),
        ),
      },
    });
    expect(await run(d)).toMatchObject({ articlesNew: 0 });
  });

  it("요약은 300자까지만 저장한다(본문은 저장하지 않음)", async () => {
    const db = testDb();
    addStocks(db);
    const { deps: d } = deps(db, {
      "https://a.example/economy.xml": {
        text: rss(
          item(
            "삼성전자 긴 기사",
            "https://n.example/long",
            ago(1),
            "가".repeat(1000),
          ),
        ),
      },
    });
    await run(d);
    expect(db.select().from(articles).get()?.summary).toHaveLength(
      SUMMARY_MAX_CHARS,
    );
  });
});

describe("기준 시각 (D-20)", () => {
  it("처음엔 24시간 전, 이후엔 지난 회차 시작 1시간 전, 최대 7일", async () => {
    const db = testDb();
    expect(computeSince(db, NOW)).toEqual(ago(24));

    db.insert(collectionRuns)
      .values({
        trigger: "manual",
        status: "success",
        startedAt: ago(3),
        since: ago(27),
      })
      .run();
    // 실패한 회차와 도는 중인 회차는 기준이 되지 않는다.
    db.insert(collectionRuns)
      .values({
        trigger: "manual",
        status: "failed",
        startedAt: ago(1),
        since: ago(4),
      })
      .run();
    expect(computeSince(db, NOW)).toEqual(ago(4));

    const later = new Date(NOW.getTime() + 30 * 24 * HOUR);
    expect(computeSince(db, later)).toEqual(
      new Date(later.getTime() - 7 * 24 * HOUR),
    );
  });
});

describe("Finnhub (미국 뉴스)", () => {
  const news = (
    headline: string,
    id: number,
    hoursAgo: number,
    summary = "",
  ): FinnhubNews => ({
    id,
    headline,
    summary,
    url: `https://finnhub.io/api/news?id=${id}`,
    source: "Yahoo",
    datetime: Math.floor(ago(hoursAgo).getTime() / 1000),
  });

  it("종목 뉴스는 그 종목에 바로 걸고, 시장 뉴스는 영어 키워드로 건다", async () => {
    const db = testDb();
    const ids = addStocks(db);
    const asked: { symbol: string; from: Date; to: Date }[] = [];
    const finnhub: FinnhubClient = {
      async companyNews(symbol, from, to) {
        asked.push({ symbol, from, to });
        return [news("Chip stocks rally", 1, 2)];
      },
      async generalNews() {
        return [
          news("Demand for AI accelerator chips surges", 2, 1),
          news("Oil prices fall", 3, 1),
        ];
      },
    };
    const { deps: d } = deps(db, {}, { feeds: [], finnhub });
    expect(await run(d)).toMatchObject({
      status: "success",
      articlesNew: 2,
      matchesNew: 2,
    });

    expect(asked).toEqual([{ symbol: "NVDA", from: ago(24), to: NOW }]);
    const rows = db
      .select({
        title: articles.title,
        publisher: articles.publisher,
        feed: articles.feed,
        stockId: articleMatches.stockId,
        path: articleMatches.keywordPath,
        kind: articleMatches.matchKind,
      })
      .from(articleMatches)
      .innerJoin(articles, eq(articles.id, articleMatches.articleId))
      .orderBy(asc(articleMatches.id))
      .all();
    expect(rows).toEqual([
      {
        title: "Chip stocks rally",
        publisher: "Yahoo",
        feed: "finnhub:company:NVDA",
        stockId: ids.nvidia,
        path: "엔비디아",
        kind: "direct",
      },
      {
        title: "Demand for AI accelerator chips surges",
        publisher: "Yahoo",
        feed: "finnhub:general",
        stockId: ids.nvidia,
        path: "엔비디아 → AI 가속기",
        kind: "keyword",
      },
    ]);
  });

  it("키가 없으면(finnhub=null) 미국 뉴스는 건너뛴다", async () => {
    const db = testDb();
    addStocks(db);
    const { deps: d } = deps(db, {}, { feeds: [] });
    await run(d);
    expect(db.select().from(collectionRuns).get()?.sources).toEqual([]);
  });
});

describe("회차 관리", () => {
  it("도중에 멈춘 회차는 시작할 때 실패로 정리한다", () => {
    const db = testDb();
    db.insert(collectionRuns)
      .values({
        trigger: "manual",
        status: "running",
        startedAt: ago(1),
        since: ago(25),
      })
      .run();
    expect(closeStaleRuns(db, NOW)).toBe(1);
    expect(db.select().from(collectionRuns).get()).toMatchObject({
      status: "failed",
      finishedAt: NOW,
    });
  });

  it("실행기: 도는 동안 다시 시작하면 null, 끝나면 다시 시작할 수 있다", async () => {
    const db = testDb();
    addStocks(db);
    let release!: () => void;
    const gate = new Promise<void>((resolve) => (release = resolve));
    const http: HttpClient = {
      async get() {
        await gate;
        return { status: 200, text: rss(), contentType: "" };
      },
    };
    const runner = createCollectionRunner({
      db,
      http,
      robots: allowAll,
      feeds: FEEDS,
      finnhub: null,
      now: () => NOW,
    });
    const first = runner.start("manual");
    expect(first).toBe(1);
    expect(runner.start("manual")).toBeNull();
    expect(runner.runningId()).toBe(1);
    release();
    await runner.idle();
    expect(runner.runningId()).toBeNull();
    expect(runner.start("scheduled")).toBe(2);
    await runner.idle();
    expect(
      db.select({ trigger: collectionRuns.trigger }).from(collectionRuns).all(),
    ).toEqual([{ trigger: "manual" }, { trigger: "scheduled" }]);
  });
});
