import { asc, desc, eq } from "drizzle-orm";
import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import { fakeDecider, seedListings } from "../../../tests/fixtures";
import { openDb, type Db } from "../db/client";
import {
  articleMatches,
  articles,
  collectionRuns,
  evaluations,
  feedHeadlines,
  stocks,
} from "../db/schema";
import { replaceKeywords } from "../keywords/generate";
import { deleteStock, registerStock } from "../stocks/repository";
import { setKeywordStatus } from "../keywords/generate";
import type { RssFeed } from "./feeds";
import type { FinnhubClient, FinnhubNews } from "./finnhub";
import type { HttpClient } from "./http";
import type { RobotsChecker } from "./robots";
import {
  closeStaleRuns,
  computeSince,
  startCollectionRun,
  sourceSinceResolver,
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

function addStocks(db: Db, at: Date = NOW) {
  const samsung = registerStock(
    db,
    {
      market: "KR",
      code: "005930",
      userInput: "삼전",
    },
    at,
  );
  const nvidia = registerStock(
    db,
    {
      market: "US",
      code: "NVDA",
      userInput: "NVDA",
      nameKo: "엔비디아",
    },
    at,
  );
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
        kind: "structure",
        path: "삼성전자 → HBM 메모리",
      },
      {
        key: 2,
        parentKey: 1,
        level: 2,
        textKo: "엔비디아",
        textEn: null,
        reason: "r",
        kind: "structure",
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
        kind: "structure",
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
      decider: null,
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

  it("발행 시각이 받은 시각보다 10분 넘게 미래면(국민일보 지면 기사) 받은 시각으로 저장한다", async () => {
    const db = testDb();
    addStocks(db);
    const { deps: d } = deps(db, {
      "https://a.example/economy.xml": {
        text: rss(
          item(
            "삼성전자 지면 기사",
            "https://n.example/f1",
            new Date(NOW.getTime() + 19 * HOUR),
          ),
          item(
            "삼성전자 조금 빠른 시계",
            "https://n.example/f2",
            new Date(NOW.getTime() + 5 * 60_000),
          ),
        ),
      },
    });
    await run(d);
    const saved = new Map(
      db
        .select()
        .from(articles)
        .all()
        .map((a) => [a.title, a.publishedAt]),
    );
    expect(saved.get("삼성전자 지면 기사")).toEqual(NOW);
    expect(saved.get("삼성전자 조금 빠른 시계")).toEqual(
      new Date(NOW.getTime() + 5 * 60_000),
    );
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

describe("수집 직후 평가 (5단계)", () => {
  const routes = {
    "https://a.example/economy.xml": {
      text: rss(
        item("삼성전자 3분기 실적", "https://n.example/1", ago(1)),
        item("엔비디아, 새 AI 가속기 공개", "https://n.example/2", ago(2)),
      ),
    },
  };

  it("새 연결을 Jev로 평가하고 회차 기록에 평가 결과를 남긴다", async () => {
    const db = testDb();
    addStocks(db);
    const decider = fakeDecider();
    const { deps: d } = deps(db, routes, { feeds: [FEEDS[0]], decider });

    expect(await run(d)).toMatchObject({ matchesNew: 3 });

    expect(decider.calls).toHaveLength(3);
    expect(db.select().from(evaluations).all()).toHaveLength(3);
    expect(db.select().from(collectionRuns).get()?.evaluation).toMatchObject({
      evaluated: 3,
      relevant: 3,
      failed: 0,
      remaining: 0,
    });
  });

  it("키가 없으면 수집만 하고 평가하지 않은 이유를 남긴다", async () => {
    const db = testDb();
    addStocks(db);
    const { deps: d } = deps(db, routes, { feeds: [FEEDS[0]] });

    expect(await run(d)).toMatchObject({ status: "success", matchesNew: 3 });
    expect(db.select().from(collectionRuns).get()?.evaluation).toMatchObject({
      evaluated: 0,
      remaining: 3,
      note: "TYPESAFE_API_KEY가 없어 평가하지 않았습니다.",
    });
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

describe("소스별 기준 시각 (D-24)", () => {
  const ok = (id: string) => ({
    id,
    label: id,
    group: id,
    fetched: 1,
    fresh: 1,
    saved: 0,
    matched: 0,
    ms: 1,
  });
  const failed = (id: string) => ({ ...ok(id), error: "HTTP 500" });

  it("지난번에 실패한 소스는 마지막으로 성공한 회차부터, 처음 보는 소스는 회차 기준 시각부터", () => {
    const db = testDb();
    const insert = (
      startedAt: Date,
      status: "success" | "partial" | "failed",
      sources: ReturnType<typeof ok>[],
    ) =>
      db
        .insert(collectionRuns)
        .values({
          trigger: "scheduled",
          status,
          startedAt,
          since: startedAt,
          sources,
        })
        .run();
    const DAY = 24 * HOUR;
    // 8일 전에 마지막으로 성공한 e는 7일 안에 성공 기록이 없다.
    insert(new Date(NOW.getTime() - 8 * DAY), "success", [ok("e")]);
    insert(ago(5), "success", [ok("a"), ok("b")]);
    insert(ago(3), "partial", [ok("a"), failed("b")]);
    insert(ago(2), "failed", [failed("a"), failed("b")]);
    insert(ago(1), "partial", [ok("a"), failed("b"), failed("c"), failed("e")]);

    const runSince = computeSince(db, NOW);
    expect(runSince).toEqual(ago(2));
    const sinceFor = sourceSinceResolver(db, NOW, runSince);
    expect(sinceFor("a")).toEqual(ago(2));
    // b는 5시간 전 회차가 마지막 성공
    expect(sinceFor("b")).toEqual(ago(6));
    // 7일 동안 한 번도 성공하지 못한 소스는 최대 기간(7일)
    expect(sinceFor("c")).toEqual(new Date(NOW.getTime() - 7 * DAY));
    expect(sinceFor("e")).toEqual(new Date(NOW.getTime() - 7 * DAY));
    // 처음 보는 소스(새 피드, 새로 등록한 미국 종목)
    expect(sinceFor("new")).toEqual(ago(2));
  });

  it("실패했던 소스가 돌아오면 그 사이 피드에 남은 기사를 받고, 쓴 기준 시각을 남긴다", async () => {
    const db = testDb();
    addStocks(db);
    // 4시간 전 회차: 두 소스 모두 성공
    await run(
      deps(
        db,
        {
          "https://a.example/economy.xml": { text: rss() },
          "https://b.example/all.xml": { text: rss() },
        },
        { now: () => ago(4) },
      ).deps,
    );
    // 2시간 전 회차: b만 실패
    await run(
      deps(
        db,
        {
          "https://a.example/economy.xml": { text: rss() },
          "https://b.example/all.xml": { status: 503, text: "" },
        },
        { now: () => ago(2) },
      ).deps,
    );
    // 지금 회차: 회차 기준 시각은 3시간 전(2시간 전 회차 − 1시간).
    // b의 3.5시간 전 기사는 b의 기준 시각(4시간 전 회차 − 1시간 = 5시간 전) 안이라 받는다.
    const item35 = item("삼성전자 b 소식", "https://n.example/b35", ago(3.5));
    const { deps: d } = deps(db, {
      "https://a.example/economy.xml": {
        text: rss(item("삼성전자 a 소식", "https://n.example/a35", ago(3.5))),
      },
      "https://b.example/all.xml": { text: rss(item35) },
    });

    await run(d);

    expect(db.select({ title: articles.title }).from(articles).all()).toEqual([
      { title: "삼성전자 b 소식" },
    ]);
    const last = db
      .select()
      .from(collectionRuns)
      .orderBy(desc(collectionRuns.id))
      .get()!;
    expect(last.since).toEqual(ago(3));
    expect(last.sources[0].since).toBeUndefined();
    expect(last.sources[1].since).toBe(ago(5).getTime());
  });
});

describe("새로 등록한 종목의 지난 기사 (D-20 보완, 2026-10-21 결정)", () => {
  function addHynix(db: Db, at: Date) {
    const r = registerStock(
      db,
      { market: "KR", code: "000660", userInput: "하닉" },
      at,
    );
    if (r.status === "not_listed") throw new Error("표본에 없음");
    return r.stock.id;
  }
  const backfillOf = (db: Db, id: number) =>
    db
      .select({ at: stocks.backfillFrom })
      .from(stocks)
      .where(eq(stocks.id, id))
      .get()?.at;
  const feed = (...items: string[]) => ({
    "https://a.example/economy.xml": { text: rss(...items) },
  });
  const titles = (db: Db) =>
    db
      .select({ title: articles.title, stockId: articleMatches.stockId })
      .from(articleMatches)
      .innerJoin(articles, eq(articles.id, articleMatches.articleId))
      .orderBy(asc(articleMatches.id))
      .all();

  it("다음 회차에서 그 종목만 등록 24시간 전 기사까지 걸고, 한 번 걸면 표시를 지운다", async () => {
    const db = testDb();
    const ids = addStocks(db, ago(10));
    // 삼성전자는 이미 수집해 온 종목
    await run(deps(db, feed(), { feeds: [FEEDS[0]], now: () => ago(5) }).deps);
    const hynix = addHynix(db, ago(1));
    setKeywordStatus(db, hynix, "ready");
    expect(backfillOf(db, hynix)).toEqual(ago(25));

    await run(
      deps(
        db,
        feed(
          item("SK하이닉스 HBM 증설", "https://n.example/h20", ago(20)),
          item("삼성전자와 SK하이닉스 협력", "https://n.example/b20", ago(20)),
          item("삼성전자 지난 소식", "https://n.example/s20", ago(20)),
          item("SK하이닉스 오래된 소식", "https://n.example/h30", ago(30)),
          item("삼성전자 새 소식", "https://n.example/s1", ago(0.5)),
        ),
        { feeds: [FEEDS[0]] },
      ).deps,
    );

    expect(titles(db)).toEqual([
      { title: "SK하이닉스 HBM 증설", stockId: hynix },
      // 기준 시각 전 기사는 새 종목에만 건다.
      { title: "삼성전자와 SK하이닉스 협력", stockId: hynix },
      { title: "삼성전자 새 소식", stockId: ids.samsung },
    ]);
    expect(backfillOf(db, hynix)).toBeNull();

    // 다음 회차부터는 다른 종목과 같은 기준 시각
    await run(
      deps(
        db,
        feed(
          item("SK하이닉스 다른 지난 소식", "https://n.example/h22", ago(22)),
        ),
        { feeds: [FEEDS[0]], now: () => new Date(NOW.getTime() + HOUR) },
      ).deps,
    );
    expect(titles(db)).toHaveLength(3);
  });

  it("키워드를 만드는 중이면 다음 회차로 미루고, 회차가 모두 실패하면 다음 회차에 다시 한다", async () => {
    const db = testDb();
    addStocks(db, ago(10));
    await run(deps(db, feed(), { feeds: [FEEDS[0]], now: () => ago(5) }).deps);
    const hynix = addHynix(db, ago(1));
    setKeywordStatus(db, hynix, "generating");
    const old = item("SK하이닉스 HBM 증설", "https://n.example/h20", ago(20));

    await run(deps(db, feed(old), { feeds: [FEEDS[0]] }).deps);
    expect(titles(db)).toEqual([]);
    expect(backfillOf(db, hynix)).toEqual(ago(25));

    setKeywordStatus(db, hynix, "ready");
    await run(
      deps(
        db,
        { "https://a.example/economy.xml": { status: 503, text: "" } },
        { feeds: [FEEDS[0]] },
      ).deps,
    );
    expect(backfillOf(db, hynix)).toEqual(ago(25));

    await run(deps(db, feed(old), { feeds: [FEEDS[0]] }).deps);
    expect(titles(db)).toEqual([
      { title: "SK하이닉스 HBM 증설", stockId: hynix },
    ]);
    expect(backfillOf(db, hynix)).toBeNull();
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

  it("새로 등록한 미국 종목은 종목 뉴스를 등록 24시간 전부터 묻는다", async () => {
    const db = testDb();
    addStocks(db, ago(10));
    const asked: { symbol: string; from: Date }[] = [];
    const finnhub: FinnhubClient = {
      async companyNews(symbol, from) {
        asked.push({ symbol, from });
        return [];
      },
      async generalNews() {
        return [];
      },
    };
    await run(deps(db, {}, { feeds: [], finnhub, now: () => ago(5) }).deps);
    const tesla = registerStock(
      db,
      { market: "US", code: "TSLA", userInput: "TSLA", nameKo: "테슬라" },
      ago(1),
    );
    if (tesla.status === "not_listed") throw new Error("표본에 없음");
    setKeywordStatus(db, tesla.stock.id, "ready");

    await run(deps(db, {}, { feeds: [], finnhub }).deps);

    expect(asked.slice(1)).toEqual([
      // 이미 수집해 온 엔비디아는 회차 기준 시각(지난 회차 1시간 전)부터
      { symbol: "NVDA", from: ago(6) },
      { symbol: "TSLA", from: ago(25) },
    ]);
  });

  it("종목에 안 걸린 기사도 제목을 남긴다: 언론사 RSS·시장 뉴스만, 미국 종목별 뉴스는 빼고(10단계)", async () => {
    const db = testDb();
    addStocks(db);
    // 지난 회차에 남은 4일 전 제목은 이번 회차에 지운다.
    db.insert(feedHeadlines)
      .values({
        titleKey: "지난제목",
        title: "지난 제목",
        publisher: "p",
        feed: "rss:a:economy",
        publishedAt: ago(96),
        seenAt: ago(96),
      })
      .run();
    const finnhub: FinnhubClient = {
      async companyNews() {
        return [news("Nvidia unveils chip", 11, 2)];
      },
      async generalNews() {
        return [news("Oil prices fall", 12, 1)];
      },
    };
    const { deps: d } = deps(
      db,
      {
        "https://a.example/economy.xml": {
          text: rss(
            item("삼성전자 3분기 실적", "https://news.example/1", ago(2)),
            item("한은 기준금리 동결", "https://news.example/2", ago(1)),
          ),
        },
      },
      { finnhub },
    );
    await run(d);

    const kept = db
      .select({ title: feedHeadlines.title, feed: feedHeadlines.feed })
      .from(feedHeadlines)
      .orderBy(asc(feedHeadlines.title))
      .all();
    expect(kept).toEqual([
      { title: "Oil prices fall", feed: "finnhub:general" },
      { title: "삼성전자 3분기 실적", feed: "rss:a:economy" },
      { title: "한은 기준금리 동결", feed: "rss:a:economy" },
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
      decider: null,
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
