import {
  and,
  asc,
  desc,
  eq,
  gte,
  inArray,
  isNotNull,
  isNull,
} from "drizzle-orm";
import type { Decider } from "@/server/ai/decider/types";
import type { Db } from "@/server/db/client";
import {
  articleMatches,
  articles,
  collectionRuns,
  keywords,
  stocks,
  type EvaluationSummary,
  type RunStatus,
  type RunTrigger,
  type SourceResult,
} from "@/server/db/schema";
import { evaluatePending } from "@/server/evaluate/run";
import { log, logError } from "@/server/log";
import type { RssFeed } from "./feeds";
import { pruneFeedHeadlines, saveFeedHeadlines } from "./feed-headlines";
import type { FinnhubClient } from "./finnhub";
import type { HttpClient } from "./http";
import { createMatcher, type MatchStock, type StockMatch } from "./match";
import type { RobotsChecker } from "./robots";
import { parseFeed } from "./rss";
import { urlKey } from "./url";

const HOUR_MS = 60 * 60 * 1000;
/** 처음 수집할 때 볼 기간(D-20) */
export const FIRST_RUN_WINDOW_MS = 24 * HOUR_MS;
/** 지난 회차와 겹쳐 볼 시간. 피드에 늦게 올라온 기사를 놓치지 않게 한다(중복은 주소로 거른다). */
export const OVERLAP_MS = HOUR_MS;
/** 오래 쉬었다가 다시 돌 때도 이보다 오래된 기사는 보지 않는다. */
export const MAX_WINDOW_MS = 7 * 24 * HOUR_MS;
/** 요약은 이만큼만 저장한다(본문은 저장하지 않음). */
export const SUMMARY_MAX_CHARS = 300;
const TITLE_MAX_CHARS = 300;
/** 동시에 처리하는 소스 수. 같은 사이트 간격은 HttpClient가 지킨다. */
const SOURCE_CONCURRENCY = 6;

/** 소스가 돌려주는 기사 한 건 */
export type CollectedItem = {
  url: string;
  title: string;
  summary: string;
  publisher: string;
  publishedAt: Date | null;
  /** 소스가 종목을 직접 지정한 경우(예: Finnhub 종목 뉴스) */
  directStockIds?: number[];
};

export type Source = {
  id: string;
  label: string;
  group: string;
  /** 받은 기사 제목을 시장 이슈 재료로 남길지(언론사 RSS·Finnhub 시장 뉴스, 10단계) */
  keepHeadlines?: boolean;
  fetch(): Promise<CollectedItem[]>;
};

export type CollectDeps = {
  db: Db;
  http: HttpClient;
  robots: RobotsChecker;
  feeds: RssFeed[];
  /** 키가 없으면 null: 미국 뉴스는 건너뛴다. */
  finnhub: FinnhubClient | null;
  /** 키가 없으면 null: 수집만 하고 평가는 미룬다(5단계). */
  decider: Decider | null;
  now?: () => Date;
};

export type RunSummary = {
  runId: number;
  status: RunStatus;
  articlesNew: number;
  matchesNew: number;
};

/**
 * 수집 한 회차(SPEC FR-4). 소스마다 따로 처리해서 한 소스가 실패해도 나머지는 계속 돈다.
 * 종목에 하나라도 걸린 기사만 저장한다. 같은 원문 주소는 한 기사로 합친다(D-19).
 */
export function startCollectionRun(
  deps: CollectDeps,
  trigger: RunTrigger,
): { runId: number; done: Promise<RunSummary> } {
  const now = deps.now ?? (() => new Date());
  const startedAt = now();
  const since = computeSince(deps.db, startedAt);
  const runId = deps.db
    .insert(collectionRuns)
    .values({ trigger, status: "running", startedAt, since })
    .returning({ id: collectionRuns.id })
    .get().id;
  const done = collect(deps, runId, startedAt, since).catch(
    (error: unknown) => {
      logError(`수집 회차 ${runId} 실패`, error);
      finishRun(deps.db, runId, now(), "failed", [], 0, 0, describe(error));
      return {
        runId,
        status: "failed" as const,
        articlesNew: 0,
        matchesNew: 0,
      };
    },
  );
  return { runId, done };
}

async function collect(
  deps: CollectDeps,
  runId: number,
  startedAt: Date,
  since: Date,
): Promise<RunSummary> {
  const now = deps.now ?? (() => new Date());
  const watched = loadMatchStocks(deps.db);
  if (watched.length === 0) {
    finishRun(
      deps.db,
      runId,
      now(),
      "success",
      [],
      0,
      0,
      "등록한 종목이 없어 수집하지 않았습니다.",
    );
    return { runId, status: "success", articlesNew: 0, matchesNew: 0 };
  }

  const sinceFor = sourceSinceResolver(deps.db, startedAt, since);
  const backfill = loadBackfill(deps.db, startedAt);
  if (backfill.size > 0) {
    log(
      `새로 등록한 종목 ${backfill.size}개는 등록 24시간 전 기사까지 연결합니다.`,
    );
  }
  const sources = buildSources(deps, watched, sinceFor, backfill, startedAt);
  const matcher = createMatcher(watched);
  const displayNames = new Map(watched.map((s) => [s.id, s.nameKo ?? s.name]));
  const results = await mapLimit(sources, SOURCE_CONCURRENCY, (source) =>
    runSource(
      deps.db,
      source,
      matcher,
      displayNames,
      { run: since, source: sinceFor(source.id) },
      backfill,
      startedAt,
    ),
  );

  try {
    pruneFeedHeadlines(deps.db, now());
  } catch (error) {
    logError("지난 피드 제목 지우기 실패", error);
  }

  const failed = results.filter((r) => r.error).length;
  const status: RunStatus =
    failed === 0 ? "success" : failed === results.length ? "failed" : "partial";
  const articlesNew = results.reduce((sum, r) => sum + r.saved, 0);
  const matchesNew = results.reduce((sum, r) => sum + r.matched, 0);
  // 회차가 돌았으면(완료·일부 실패) 새 종목의 지난 기사 연결은 끝난 것으로 본다. 모두 실패하면 다음 회차에 다시.
  if (status !== "failed") clearBackfill(deps.db, backfill);
  log(
    `수집 회차 ${runId}: ${status}, 소스 ${results.length}개(실패 ${failed}), 새 기사 ${articlesNew}건, 새 연결 ${matchesNew}건`,
  );

  // 수집 직후 새 연결(과 지난번에 못 한 것)을 평가한다(5단계 결정). 평가가 실패해도 수집 결과는 남긴다.
  let evaluation: EvaluationSummary | null = null;
  try {
    evaluation = await evaluatePending({
      db: deps.db,
      decider: deps.decider,
      now,
    });
    log(
      `평가 회차 ${runId}: ${evaluation.evaluated}건(기준 이상 ${evaluation.relevant}, 실패 ${evaluation.failed}, 남김 ${evaluation.remaining}), $${evaluation.costUsd.toFixed(5)}${evaluation.note ? `, ${evaluation.note}` : ""}`,
    );
  } catch (error) {
    logError(`평가 회차 ${runId} 실패`, error);
  }
  finishRun(
    deps.db,
    runId,
    now(),
    status,
    results,
    articlesNew,
    matchesNew,
    null,
    evaluation,
  );
  return { runId, status, articlesNew, matchesNew };
}

function buildSources(
  deps: CollectDeps,
  watched: MatchStock[],
  sinceFor: (sourceId: string) => Date,
  backfill: Backfill,
  startedAt: Date,
): Source[] {
  const sources: Source[] = deps.feeds.map((feed) => ({
    id: feed.id,
    label: `${feed.publisher} ${feed.section}`,
    group: feed.publisher,
    keepHeadlines: true,
    async fetch() {
      if (!(await deps.robots.allowed(feed.url))) {
        throw new Error(
          "robots.txt가 막고 있거나 확인하지 못해 건너뛰었습니다.",
        );
      }
      const response = await deps.http.get(feed.url);
      if (response.status !== 200) throw new Error(`HTTP ${response.status}`);
      return parseFeed(response.text, startedAt).map((item) => ({
        url: item.link,
        title: item.title,
        summary: item.summary,
        publisher: feed.publisher,
        publishedAt: item.publishedAt,
      }));
    },
  }));

  const finnhub = deps.finnhub;
  if (finnhub) {
    for (const stock of watched.filter((s) => s.market === "US")) {
      const id = `finnhub:company:${stock.code}`;
      sources.push({
        id,
        label: `Finnhub ${stock.nameKo ?? stock.name}`,
        group: "Finnhub",
        async fetch() {
          const since = sinceFor(id);
          const from = backfill.get(stock.id)?.from;
          const news = await finnhub.companyNews(
            stock.code,
            from && from < since ? from : since,
            startedAt,
          );
          return news.map((n) => ({
            url: n.url,
            title: n.headline,
            summary: n.summary,
            publisher: n.source || "Finnhub",
            publishedAt: new Date(n.datetime * 1000),
            directStockIds: [stock.id],
          }));
        },
      });
    }
    sources.push({
      id: "finnhub:general",
      label: "Finnhub 시장 뉴스",
      group: "Finnhub",
      // 미국 종목별 뉴스는 한 종목으로 쏠리지 않게 남기지 않는다.
      keepHeadlines: true,
      async fetch() {
        const news = await finnhub.generalNews();
        return news.map((n) => ({
          url: n.url,
          title: n.headline,
          summary: n.summary,
          publisher: n.source || "Finnhub",
          publishedAt: new Date(n.datetime * 1000),
        }));
      },
    });
  }
  return sources;
}

async function runSource(
  db: Db,
  source: Source,
  matcher: (text: string) => StockMatch[],
  displayNames: Map<number, string>,
  sinces: { run: Date; source: Date },
  backfill: Backfill,
  seenAt: Date,
): Promise<SourceResult> {
  const started = Date.now();
  const since = sinces.source;
  const backfillFrom = earliest(backfill);
  const result: SourceResult = {
    id: source.id,
    label: source.label,
    group: source.group,
    fetched: 0,
    fresh: 0,
    saved: 0,
    matched: 0,
    ...(since.getTime() !== sinces.run.getTime()
      ? { since: since.getTime() }
      : {}),
    ms: 0,
  };
  try {
    const items = await source.fetch();
    result.fetched = items.length;
    if (source.keepHeadlines) {
      // 제목 보관이 실패해도 기사 수집·연결은 계속한다.
      try {
        saveFeedHeadlines(db, source.id, items, seenAt);
      } catch (error) {
        logError(`피드 제목 저장 실패: ${source.id}`, error);
      }
    }
    const keep: {
      item: CollectedItem;
      key: string;
      publishedAt: Date;
      matches: StockMatch[];
    }[] = [];
    for (const item of items) {
      const key = urlKey(item.url);
      if (!key || !item.title.trim()) continue;
      // 발행 시각이 없는 피드(한겨레 등)는 처음 받은 시각으로 본다.
      const publishedAt = item.publishedAt ?? seenAt;
      const fresh = publishedAt >= since;
      if (!fresh && !(backfillFrom && publishedAt >= backfillFrom)) continue;
      if (fresh) result.fresh += 1;
      // 피드가 본문 전체를 요약 자리에 넣기도 해서, 저장하는 요약(앞 300자)까지만 보고 건다.
      const clipped = {
        ...item,
        summary: clip(item.summary, SUMMARY_MAX_CHARS),
      };
      let matches = mergeMatches(clipped, matcher, displayNames);
      // 기준 시각 전 기사는 새로 등록한 종목에만 건다(D-20 보완).
      if (!fresh) {
        matches = matches.filter((m) => {
          const from = backfill.get(m.stockId)?.from;
          return from !== undefined && publishedAt >= from;
        });
      }
      if (matches.length > 0)
        keep.push({ item: clipped, key, publishedAt, matches });
    }
    const saved = saveItems(db, source.id, keep, seenAt);
    result.saved = saved.articles;
    result.matched = saved.matches;
  } catch (error) {
    result.error = describe(error);
  }
  result.ms = Date.now() - started;
  return result;
}

/** 소스가 직접 지정한 종목은 종목 자체(0단계)로, 나머지는 제목+요약으로 찾는다. */
function mergeMatches(
  item: CollectedItem,
  matcher: (text: string) => StockMatch[],
  displayNames: Map<number, string>,
): StockMatch[] {
  const byStock = new Map<number, StockMatch>();
  for (const m of matcher(`${item.title} ${item.summary}`))
    byStock.set(m.stockId, m);
  for (const stockId of item.directStockIds ?? []) {
    byStock.set(stockId, {
      stockId,
      keywordPath: displayNames.get(stockId) ?? "",
      keywordReason: null,
      matchKind: "direct",
      level: 0,
    });
  }
  return [...byStock.values()];
}

function saveItems(
  db: Db,
  feedId: string,
  keep: {
    item: CollectedItem;
    key: string;
    publishedAt: Date;
    matches: StockMatch[];
  }[],
  at: Date,
): { articles: number; matches: number } {
  if (keep.length === 0) return { articles: 0, matches: 0 };
  return db.transaction((tx) => {
    let newArticles = 0;
    let newMatches = 0;
    for (const { item, key, publishedAt, matches } of keep) {
      let articleId = tx
        .select({ id: articles.id })
        .from(articles)
        .where(eq(articles.urlKey, key))
        .get()?.id;
      if (articleId === undefined) {
        articleId = tx
          .insert(articles)
          .values({
            urlKey: key,
            url: item.url.trim(),
            title: clip(item.title, TITLE_MAX_CHARS),
            summary: item.summary || null,
            publisher: item.publisher,
            feed: feedId,
            publishedAt,
            collectedAt: at,
          })
          .returning({ id: articles.id })
          .get().id;
        newArticles += 1;
      }
      for (const m of matches) {
        const inserted = tx
          .insert(articleMatches)
          .values({
            articleId,
            stockId: m.stockId,
            keywordPath: m.keywordPath,
            keywordReason: m.keywordReason,
            matchKind: m.matchKind,
            level: m.level,
            createdAt: at,
          })
          .onConflictDoNothing()
          .run();
        newMatches += inserted.changes;
      }
    }
    return { articles: newArticles, matches: newMatches };
  });
}

/** 삭제하지 않은 종목과 키워드 트리 */
export function loadMatchStocks(db: Db): MatchStock[] {
  const active = db
    .select()
    .from(stocks)
    .where(isNull(stocks.deletedAt))
    .orderBy(asc(stocks.id))
    .all();
  if (active.length === 0) return [];
  const rows = db
    .select()
    .from(keywords)
    .where(
      inArray(
        keywords.stockId,
        active.map((s) => s.id),
      ),
    )
    .orderBy(asc(keywords.level), asc(keywords.id))
    .all();
  return active.map((s) => ({
    id: s.id,
    market: s.market,
    code: s.code,
    name: s.name,
    nameKo: s.nameKo,
    keywords: rows
      .filter((k) => k.stockId === s.id)
      .map((k) => ({
        level: k.level,
        textKo: k.textKo,
        textEn: k.textEn,
        path: k.path,
        reason: k.reason,
      })),
  }));
}

/**
 * 이번 회차에서 볼 기사의 기준 시각(D-20).
 * 처음엔 최근 24시간, 이후엔 지난 회차(성공·일부 실패) 시작 1시간 전부터. 최대 7일.
 */
export function computeSince(db: Db, now: Date): Date {
  const last = db
    .select({ startedAt: collectionRuns.startedAt })
    .from(collectionRuns)
    .where(inArray(collectionRuns.status, ["success", "partial"]))
    .orderBy(desc(collectionRuns.startedAt))
    .limit(1)
    .get();
  const floor = now.getTime() - MAX_WINDOW_MS;
  const from = last
    ? last.startedAt.getTime() - OVERLAP_MS
    : now.getTime() - FIRST_RUN_WINDOW_MS;
  return new Date(Math.max(from, floor));
}

/** 새로 등록한 종목: 종목 id → 이번 회차에서 연결할 가장 이른 발행 시각(from)과 DB에 적힌 값(stored) */
type Backfill = Map<number, { from: Date; stored: Date }>;

/**
 * 새로 등록하거나 되살린 종목(D-20 보완, 2026-10-21 결정): 이번 회차에서 등록 24시간 전 기사까지 연결한다.
 * 키워드를 만드는 중인 종목은 다음 회차로 미룬다(키워드로 걸 기사를 놓치지 않게). 최대 7일 전까지.
 */
export function loadBackfill(db: Db, now: Date): Backfill {
  const floor = now.getTime() - MAX_WINDOW_MS;
  const rows = db
    .select({
      id: stocks.id,
      stored: stocks.backfillFrom,
      status: stocks.keywordsStatus,
    })
    .from(stocks)
    .where(and(isNull(stocks.deletedAt), isNotNull(stocks.backfillFrom)))
    .all();
  const backfill: Backfill = new Map();
  for (const row of rows) {
    if (!row.stored || row.status === "pending" || row.status === "generating")
      continue;
    backfill.set(row.id, {
      from: new Date(Math.max(row.stored.getTime(), floor)),
      stored: row.stored,
    });
  }
  return backfill;
}

/** 이번 회차에서 지난 기사를 연결한 종목의 표시를 지운다. 그사이 다시 등록돼 값이 바뀐 종목은 남긴다. */
function clearBackfill(db: Db, backfill: Backfill): void {
  for (const [stockId, { stored }] of backfill) {
    db.update(stocks)
      .set({ backfillFrom: null })
      .where(and(eq(stocks.id, stockId), eq(stocks.backfillFrom, stored)))
      .run();
  }
}

function earliest(backfill: Backfill): Date | null {
  let min: Date | null = null;
  for (const { from } of backfill.values()) {
    if (!min || from < min) min = from;
  }
  return min;
}

/**
 * 소스마다 볼 기사의 기준 시각(D-24): 그 소스가 마지막으로 성공한 회차 시작 1시간 전.
 * 지난번에 실패한 소스는 이번 회차 기준 시각보다 앞에서부터 받아, 피드에 남은 기사를 놓치지 않는다.
 * 처음 보는 소스(새 피드·새로 등록한 미국 종목)는 회차 기준 시각, 최근 7일 동안 한 번도 성공하지 못한 소스는 7일 전부터.
 */
export function sourceSinceResolver(
  db: Db,
  now: Date,
  runSince: Date,
): (sourceId: string) => Date {
  const floor = new Date(now.getTime() - MAX_WINDOW_MS);
  const runs = db
    .select({
      startedAt: collectionRuns.startedAt,
      sources: collectionRuns.sources,
    })
    .from(collectionRuns)
    .where(
      and(
        inArray(collectionRuns.status, ["success", "partial", "failed"]),
        gte(collectionRuns.startedAt, floor),
      ),
    )
    .orderBy(desc(collectionRuns.startedAt))
    .all();
  const lastOk = new Map<string, Date>();
  const seen = new Set<string>();
  for (const run of runs) {
    for (const source of run.sources) {
      seen.add(source.id);
      if (!source.error && !lastOk.has(source.id)) {
        lastOk.set(source.id, run.startedAt);
      }
    }
  }
  return (sourceId) => {
    const ok = lastOk.get(sourceId);
    if (!ok) return seen.has(sourceId) ? floor : runSince;
    const from = Math.max(ok.getTime() - OVERLAP_MS, floor.getTime());
    return new Date(Math.min(from, runSince.getTime()));
  };
}

function finishRun(
  db: Db,
  runId: number,
  at: Date,
  status: RunStatus,
  sources: SourceResult[],
  articlesNew: number,
  matchesNew: number,
  note: string | null,
  evaluation: EvaluationSummary | null = null,
): void {
  db.update(collectionRuns)
    .set({
      status,
      finishedAt: at,
      sources,
      articlesNew,
      matchesNew,
      note,
      evaluation,
    })
    .where(eq(collectionRuns.id, runId))
    .run();
}

/** worker가 도중에 꺼져 '수집 중'으로 남은 회차를 실패로 정리한다(시작할 때 부른다). */
export function closeStaleRuns(db: Db, now: Date = new Date()): number {
  return db
    .update(collectionRuns)
    .set({
      status: "failed",
      finishedAt: now,
      note: "worker가 수집 도중 멈췄습니다.",
    })
    .where(eq(collectionRuns.status, "running"))
    .run().changes;
}

async function mapLimit<T, R>(
  items: T[],
  limit: number,
  fn: (item: T) => Promise<R>,
): Promise<R[]> {
  const results = new Array<R>(items.length);
  let next = 0;
  const workers = Array.from(
    { length: Math.min(limit, items.length) },
    async () => {
      while (next < items.length) {
        const index = next++;
        results[index] = await fn(items[index]);
      }
    },
  );
  await Promise.all(workers);
  return results;
}

function clip(text: string, max: number): string {
  const trimmed = text.trim();
  return trimmed.length > max ? `${trimmed.slice(0, max - 1)}…` : trimmed;
}

function describe(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
