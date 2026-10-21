import { and, desc, eq, gte, inArray, isNull } from "drizzle-orm";
import type { Db } from "@/server/db/client";
import {
  articleMatches,
  articles,
  evaluations,
  IMPACTS,
  stocks,
  type Impact,
  type MatchKind,
} from "@/server/db/schema";
import { readSetting } from "@/server/settings";

/** "더 보기" 한 번에 늘리는 카드 수(D-25) */
export const TIMELINE_PAGE_SIZE = 30;
/** 주소를 손으로 바꿔도 조회가 지나치게 무거워지지 않게 하는 상한 */
export const TIMELINE_MAX = 600;
/** 제목이 같으면 한 카드로 묶는 기간(2026-10-21 결정) */
export const SAME_TITLE_WINDOW_MS = 24 * 60 * 60 * 1000;
/** 기사를 이만큼씩 읽어 카드를 만든다. */
const SCAN_BATCH = 200;

export type TimelineFilter = { stockId: number | null; impact: Impact | null };

export type TimelineStock = {
  stockId: number;
  name: string;
  relevance: number;
  impact: Impact;
  matchKind: MatchKind;
  path: string;
};

export type TimelineCard = {
  articleId: number;
  title: string;
  url: string;
  publisher: string;
  publishedAt: Date;
  /** 관련도 기준 이상인 종목, 관련도 높은 순(D-26) */
  stocks: TimelineStock[];
  /** 제목이 같은 다른 언론사 기사(24시간 안, 최근 것부터) */
  others: { articleId: number; publisher: string; url: string }[];
};

/** 주소의 검색 값(stock, impact, limit)을 읽는다. 없거나 맞지 않는 값은 무시한다. */
export function parseTimelineParams(
  params: Record<string, string | string[] | undefined>,
  activeStockIds: number[],
): { filter: TimelineFilter; limit: number } {
  const one = (key: string) => {
    const value = params[key];
    return Array.isArray(value) ? value[0] : value;
  };
  const stockId = Number(one("stock"));
  const impact = one("impact");
  const limit = Number(one("limit"));
  return {
    filter: {
      stockId: activeStockIds.includes(stockId) ? stockId : null,
      impact: (IMPACTS as readonly string[]).includes(impact ?? "")
        ? (impact as Impact)
        : null,
    },
    limit: Number.isFinite(limit)
      ? Math.min(
          TIMELINE_MAX,
          Math.max(
            TIMELINE_PAGE_SIZE,
            Math.ceil(limit / TIMELINE_PAGE_SIZE) * TIMELINE_PAGE_SIZE,
          ),
        )
      : TIMELINE_PAGE_SIZE,
  };
}

/**
 * 통합 타임라인(SPEC FR-7): 관련도 기준 이상인 기사를 최신순으로, 기사 하나에 카드 하나(D-26).
 * 종목 필터는 그 종목이, 영향 필터는 그 영향인 종목이 카드에 있으면 통과한다(둘 다면 같은 종목).
 * 제목이 완전히 같고 24시간 안에 나온 기사는 최근 것 하나로 묶는다(화면에서만). 삭제한 종목은 빼고(D-11), web은 읽기만 한다.
 */
export function readTimeline(
  db: Db,
  options: { filter: TimelineFilter; limit: number },
): { cards: TimelineCard[]; hasMore: boolean; minRelevance: number } {
  const { filter, limit } = options;
  const minRelevance = readSetting(db, "evaluation.minRelevance");
  const conditions = [
    isNull(stocks.deletedAt),
    gte(evaluations.relevance, minRelevance),
    ...(filter.stockId ? [eq(evaluations.stockId, filter.stockId)] : []),
    ...(filter.impact ? [eq(evaluations.impact, filter.impact)] : []),
  ];
  const page = (offset: number) =>
    db
      .selectDistinct({
        id: articles.id,
        title: articles.title,
        url: articles.url,
        publisher: articles.publisher,
        publishedAt: articles.publishedAt,
      })
      .from(evaluations)
      .innerJoin(articles, eq(articles.id, evaluations.articleId))
      .innerJoin(stocks, eq(stocks.id, evaluations.stockId))
      .where(and(...conditions))
      .orderBy(desc(articles.publishedAt), desc(articles.id))
      .limit(SCAN_BATCH)
      .offset(offset)
      .all();

  const cards: TimelineCard[] = [];
  const byTitle = new Map<string, TimelineCard[]>();
  scan: for (let offset = 0; ; offset += SCAN_BATCH) {
    const rows = page(offset);
    for (const row of rows) {
      // 보여 줄 카드 중 가장 오래된 것과도 24시간 넘게 떨어지면 더 묶일 기사가 없다.
      const last = cards[limit - 1];
      if (
        cards.length > limit &&
        last.publishedAt.getTime() - row.publishedAt.getTime() >
          SAME_TITLE_WINDOW_MS
      ) {
        break scan;
      }
      const key = row.title.replace(/\s+/g, " ").trim();
      const group = byTitle
        .get(key)
        ?.find(
          (card) =>
            card.publishedAt.getTime() - row.publishedAt.getTime() <=
            SAME_TITLE_WINDOW_MS,
        );
      if (group) {
        group.others.push({
          articleId: row.id,
          publisher: row.publisher,
          url: row.url,
        });
        continue;
      }
      const card: TimelineCard = {
        articleId: row.id,
        title: row.title,
        url: row.url,
        publisher: row.publisher,
        publishedAt: row.publishedAt,
        stocks: [],
        others: [],
      };
      cards.push(card);
      byTitle.set(key, [...(byTitle.get(key) ?? []), card]);
    }
    if (rows.length < SCAN_BATCH) break;
  }

  const shown = cards.slice(0, limit);
  if (shown.length > 0) {
    const rows = db
      .select({
        articleId: evaluations.articleId,
        stockId: stocks.id,
        name: stocks.name,
        nameKo: stocks.nameKo,
        relevance: evaluations.relevance,
        impact: evaluations.impact,
        matchKind: articleMatches.matchKind,
        path: articleMatches.keywordPath,
      })
      .from(evaluations)
      .innerJoin(articleMatches, eq(articleMatches.id, evaluations.matchId))
      .innerJoin(stocks, eq(stocks.id, evaluations.stockId))
      .where(
        and(
          inArray(
            evaluations.articleId,
            shown.map((card) => card.articleId),
          ),
          isNull(stocks.deletedAt),
          gte(evaluations.relevance, minRelevance),
        ),
      )
      .all();
    const byArticle = new Map(shown.map((card) => [card.articleId, card]));
    for (const row of rows) {
      byArticle.get(row.articleId)?.stocks.push({
        stockId: row.stockId,
        name: row.nameKo ?? row.name,
        relevance: row.relevance,
        impact: row.impact,
        matchKind: row.matchKind,
        path: row.path,
      });
    }
    for (const card of shown) {
      card.stocks.sort(
        (a, b) => b.relevance - a.relevance || a.stockId - b.stockId,
      );
    }
  }
  return { cards: shown, hasMore: cards.length > limit, minRelevance };
}
