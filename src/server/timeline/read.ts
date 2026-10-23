import {
  and,
  desc,
  eq,
  gte,
  inArray,
  isNull,
  max,
  or,
  type SQL,
} from "drizzle-orm";
import type { Db } from "@/server/db/client";
import {
  articleMatches,
  articles,
  evaluations,
  IMPACTS,
  keywords,
  stocks,
  topics,
  type Impact,
  type ItemKind,
  type Market,
  type MatchKind,
  type TopicSource,
} from "@/server/db/schema";
import { PATH_SEPARATOR } from "@/server/keywords/tree";
import { readSetting } from "@/server/settings";
import {
  isSameNews,
  SAME_NEWS_WINDOW_MS,
  titleFeatures,
  type TitleFeatures,
} from "@/server/similar-title";

/** "더 보기" 한 번에 늘리는 카드 수(D-25) */
export const TIMELINE_PAGE_SIZE = 30;
/** 주소를 손으로 바꿔도 조회가 지나치게 무거워지지 않게 하는 상한 */
export const TIMELINE_MAX = 600;
/** 같은 소식(비슷한 제목)을 한 카드로 묶는 기간(11단계, 평가 복사와 같은 기준) */
export const SAME_TITLE_WINDOW_MS = SAME_NEWS_WINDOW_MS;
/** 기사를 이만큼씩 읽어 카드를 만든다. */
const SCAN_BATCH = 200;
const HOUR_MS = 60 * 60 * 1000;
/** 자동 토픽: 새 기사가 이만큼 없으면 '지난 이슈'로 접고, 이만큼 지나면 숨긴다(11단계 결정) */
export const TOPIC_FADE_MS = 12 * HOUR_MS;
export const TOPIC_HIDE_MS = 72 * HOUR_MS;

export type TimelineFilter = {
  /** 종목·시장 항목 하나 */
  stockId: number | null;
  /** 주제 하나(사용자 주제·자동 토픽, 증시별 항목을 모두) */
  topicId: number | null;
  impact: Impact | null;
};

export type TimelineStock = {
  stockId: number;
  kind: ItemKind;
  market: Market;
  /** 주제 항목: 토픽과 그 출처 */
  topicId: number | null;
  topicSource: TopicSource | null;
  name: string;
  relevance: number;
  /** 영향도. 11단계 전에 평가한 기사는 없음 */
  magnitude: number | null;
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
  /** 기준 이상인 항목(같은 소식으로 묶인 기사의 항목까지), 관련도 높은 순(D-26) */
  stocks: TimelineStock[];
  /** 같은 소식(비슷한 제목)의 다른 언론사 기사(24시간 안, 최근 것부터) */
  others: { articleId: number; publisher: string; url: string }[];
};

/** 주소의 검색 값(stock, topic, impact, limit)을 읽는다. 없거나 맞지 않는 값은 무시한다. */
export function parseTimelineParams(
  params: Record<string, string | string[] | undefined>,
  activeStockIds: number[],
  topicIds: number[] = [],
): { filter: TimelineFilter; limit: number } {
  const one = (key: string) => {
    const value = params[key];
    return Array.isArray(value) ? value[0] : value;
  };
  const stockId = Number(one("stock"));
  const topicId = Number(one("topic"));
  const impact = one("impact");
  const limit = Number(one("limit"));
  return {
    filter: {
      stockId: activeStockIds.includes(stockId) ? stockId : null,
      topicId: topicIds.includes(topicId) ? topicId : null,
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

/** 화면에 보일 평가: 관련도와 영향도가 둘 다 기준 이상(11단계). 11단계 전 평가(영향도 없음)는 관련도만 본다. */
export function displayRule(db: Db): SQL {
  return and(
    gte(evaluations.relevance, readSetting(db, "evaluation.minRelevance")),
    or(
      isNull(evaluations.magnitude),
      gte(evaluations.magnitude, readSetting(db, "evaluation.minMagnitude")),
    ),
  )!;
}

/**
 * 통합 타임라인(SPEC FR-7): 표시 기준 이상인 기사를 최신순으로, 기사 하나에 카드 하나(D-26).
 * 종목·주제 필터는 그 항목이, 영향 필터는 그 영향인 항목이 카드에 있으면 통과한다.
 * 같은 소식(비슷한 제목)을 24시간 안에 낸 기사는 최근 것 하나로 묶고 항목은 모은다(화면에서만, 11단계).
 * 삭제한 항목은 빼고(D-11), web은 읽기만 한다.
 */
export function readTimeline(
  db: Db,
  options: { filter: TimelineFilter; limit: number },
): {
  cards: TimelineCard[];
  hasMore: boolean;
  minRelevance: number;
  minMagnitude: number;
} {
  const { filter, limit } = options;
  const minRelevance = readSetting(db, "evaluation.minRelevance");
  const minMagnitude = readSetting(db, "evaluation.minMagnitude");
  const conditions = [
    isNull(stocks.deletedAt),
    displayRule(db),
    ...(filter.stockId ? [eq(evaluations.stockId, filter.stockId)] : []),
    ...(filter.topicId ? [eq(stocks.topicId, filter.topicId)] : []),
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
  const features = new Map<TimelineCard, TitleFeatures>();
  /** 카드마다 묶인 기사 id(대표 포함): 항목을 모을 때 쓴다 */
  const members = new Map<TimelineCard, number[]>();
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
      const mine = titleFeatures(row.title);
      let group: TimelineCard | undefined;
      for (let i = cards.length - 1; i >= 0; i--) {
        const card = cards[i];
        if (
          card.publishedAt.getTime() - row.publishedAt.getTime() >
          SAME_TITLE_WINDOW_MS
        )
          break;
        if (isSameNews(mine, features.get(card)!)) {
          group = card;
          break;
        }
      }
      if (group) {
        group.others.push({
          articleId: row.id,
          publisher: row.publisher,
          url: row.url,
        });
        members.get(group)!.push(row.id);
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
      features.set(card, mine);
      members.set(card, [row.id]);
    }
    if (rows.length < SCAN_BATCH) break;
  }

  const shown = cards.slice(0, limit);
  if (shown.length > 0) {
    const cardOf = new Map<number, TimelineCard>();
    for (const card of shown) {
      for (const id of members.get(card) ?? []) cardOf.set(id, card);
    }
    const rows = db
      .select({
        articleId: evaluations.articleId,
        stockId: stocks.id,
        kind: stocks.kind,
        market: stocks.market,
        topicId: stocks.topicId,
        topicSource: topics.source,
        name: stocks.name,
        nameKo: stocks.nameKo,
        relevance: evaluations.relevance,
        magnitude: evaluations.magnitude,
        impact: evaluations.impact,
        matchKind: articleMatches.matchKind,
        path: articleMatches.keywordPath,
      })
      .from(evaluations)
      .innerJoin(articleMatches, eq(articleMatches.id, evaluations.matchId))
      .innerJoin(stocks, eq(stocks.id, evaluations.stockId))
      .leftJoin(topics, eq(topics.id, stocks.topicId))
      .where(
        and(
          inArray(evaluations.articleId, [...cardOf.keys()]),
          isNull(stocks.deletedAt),
          displayRule(db),
        ),
      )
      .all();
    for (const row of rows) {
      const card = cardOf.get(row.articleId);
      if (!card) continue;
      const item: TimelineStock = {
        stockId: row.stockId,
        kind: row.kind,
        market: row.market,
        topicId: row.topicId,
        topicSource: row.topicSource,
        name: row.nameKo ?? row.name,
        relevance: row.relevance,
        magnitude: row.magnitude,
        impact: row.impact,
        matchKind: row.matchKind,
        path: row.path,
      };
      // 같은 소식으로 묶인 기사들이 같은 항목에 걸렸으면 관련도가 높은 것 하나만
      const same = card.stocks.findIndex((s) => s.stockId === row.stockId);
      if (same === -1) card.stocks.push(item);
      else if (card.stocks[same].relevance < item.relevance)
        card.stocks[same] = item;
    }
    for (const card of shown) {
      card.stocks.sort(
        (a, b) => b.relevance - a.relevance || a.stockId - b.stockId,
      );
    }
  }
  return {
    cards: shown,
    hasMore: cards.length > limit,
    minRelevance,
    minMagnitude,
  };
}

export type TopicCard = {
  topicId: number;
  name: string;
  summary: string | null;
  /** 12시간 넘게 새 기사가 없어 '지난 이슈'로 접은 토픽 */
  faded: boolean;
  lastArticleAt: Date;
  /** 표시 기준 이상인 기사 수(최근 24시간)와 전체 */
  shown24h: number;
  shownTotal: number;
  /** 켜진 증시마다 표시 기준 이상 기사들의 호재/악재(많은 쪽) */
  impacts: { market: Market; impact: Impact }[];
  /** 이 토픽 가지가 붙은 내 종목과, 그 가지로 걸린 기준 이상 기사들의 호재/악재(없으면 null) */
  related: { stockId: number; name: string; impact: Impact | null }[];
};

type ScoredImpact = {
  impact: Impact;
  relevance: number;
  magnitude: number | null;
};

/** 많은 쪽 호재/악재(같으면 점수가 가장 높은 평가 쪽, 11단계 결정) */
export function majorityImpact(items: ScoredImpact[]): Impact | null {
  if (items.length === 0) return null;
  const counts = new Map<Impact, number>();
  for (const item of items)
    counts.set(item.impact, (counts.get(item.impact) ?? 0) + 1);
  const top = Math.max(...counts.values());
  const tied = [...counts].filter(([, n]) => n === top).map(([i]) => i);
  if (tied.length === 1) return tied[0];
  const strength = (s: ScoredImpact) => s.relevance + (s.magnitude ?? 0);
  return [...items]
    .filter((s) => tied.includes(s.impact))
    .sort((a, b) => strength(b) - strength(a))[0].impact;
}

/**
 * 타임라인 맨 위 '지금 시장 이슈'(11단계): 표시 기준 이상 기사가 하나라도 있는 자동 토픽을 그런 기사가 많은 순으로.
 * 12시간 동안 새 기사가 없으면 '지난 이슈'로 접고, 3일이 지나면 숨긴다.
 */
export function readTopicCards(db: Db, now: Date = new Date()): TopicCard[] {
  const autoTopics = db
    .select()
    .from(topics)
    .where(and(eq(topics.source, "auto"), isNull(topics.deletedAt)))
    .all();
  if (autoTopics.length === 0) return [];
  const lenses = db
    .select({ id: stocks.id, topicId: stocks.topicId, market: stocks.market })
    .from(stocks)
    .where(
      and(
        eq(stocks.kind, "topic"),
        inArray(
          stocks.topicId,
          autoTopics.map((t) => t.id),
        ),
        isNull(stocks.deletedAt),
      ),
    )
    .all();
  if (lenses.length === 0) return [];
  const lensIds = lenses.map((lens) => lens.id);
  const lensOf = new Map(lenses.map((lens) => [lens.id, lens]));

  const lastByLens = db
    .select({ stockId: articleMatches.stockId, at: max(articles.publishedAt) })
    .from(articleMatches)
    .innerJoin(articles, eq(articles.id, articleMatches.articleId))
    .where(inArray(articleMatches.stockId, lensIds))
    .groupBy(articleMatches.stockId)
    .all();
  const lastByTopic = new Map<number, number>();
  for (const row of lastByLens) {
    const topicId = lensOf.get(row.stockId)?.topicId;
    if (!topicId || !row.at) continue;
    const at = new Date(row.at).getTime();
    lastByTopic.set(topicId, Math.max(lastByTopic.get(topicId) ?? 0, at));
  }

  const shownRows = db
    .select({
      stockId: evaluations.stockId,
      articleId: evaluations.articleId,
      publishedAt: articles.publishedAt,
      relevance: evaluations.relevance,
      magnitude: evaluations.magnitude,
      impact: evaluations.impact,
    })
    .from(evaluations)
    .innerJoin(articles, eq(articles.id, evaluations.articleId))
    .where(and(inArray(evaluations.stockId, lensIds), displayRule(db)))
    .all();

  // 붙은 내 종목: 토픽 가지(1단계, topic_id)가 있는 종목
  const attached = db
    .select({
      stockId: stocks.id,
      name: stocks.name,
      nameKo: stocks.nameKo,
      topicId: keywords.topicId,
      path: keywords.path,
    })
    .from(keywords)
    .innerJoin(stocks, eq(stocks.id, keywords.stockId))
    .where(
      and(
        inArray(
          keywords.topicId,
          autoTopics.map((t) => t.id),
        ),
        eq(keywords.level, 1),
        eq(stocks.kind, "stock"),
        isNull(stocks.deletedAt),
      ),
    )
    .all();
  const attachedShown = attached.length
    ? db
        .select({
          stockId: evaluations.stockId,
          path: articleMatches.keywordPath,
          relevance: evaluations.relevance,
          magnitude: evaluations.magnitude,
          impact: evaluations.impact,
        })
        .from(evaluations)
        .innerJoin(articleMatches, eq(articleMatches.id, evaluations.matchId))
        .where(
          and(
            inArray(
              evaluations.stockId,
              attached.map((a) => a.stockId),
            ),
            displayRule(db),
          ),
        )
        .all()
    : [];

  const cards: TopicCard[] = [];
  for (const topic of autoTopics) {
    const last = lastByTopic.get(topic.id);
    if (last === undefined) continue;
    const age = now.getTime() - last;
    if (age > TOPIC_HIDE_MS) continue;
    const mine = shownRows.filter(
      (row) => lensOf.get(row.stockId)?.topicId === topic.id,
    );
    if (mine.length === 0) continue;
    const articleIds = new Set(mine.map((row) => row.articleId));
    const recent = new Set(
      mine
        .filter(
          (row) => now.getTime() - row.publishedAt.getTime() <= 24 * HOUR_MS,
        )
        .map((row) => row.articleId),
    );
    const markets = [
      ...new Set(mine.map((row) => lensOf.get(row.stockId)!.market)),
    ];
    cards.push({
      topicId: topic.id,
      name: topic.name,
      summary: topic.summary,
      faded: age > TOPIC_FADE_MS,
      lastArticleAt: new Date(last),
      shown24h: recent.size,
      shownTotal: articleIds.size,
      impacts: markets.sort().flatMap((market) => {
        const impact = majorityImpact(
          mine.filter((row) => lensOf.get(row.stockId)!.market === market),
        );
        return impact ? [{ market, impact }] : [];
      }),
      related: attached
        .filter((row) => row.topicId === topic.id)
        .map((row) => {
          const prefix = `${row.path}${PATH_SEPARATOR}`;
          return {
            stockId: row.stockId,
            name: row.nameKo ?? row.name,
            impact: majorityImpact(
              attachedShown.filter(
                (e) =>
                  e.stockId === row.stockId &&
                  (e.path === row.path || e.path.startsWith(prefix)),
              ),
            ),
          };
        }),
    });
  }
  return cards.sort(
    (a, b) =>
      Number(a.faded) - Number(b.faded) ||
      b.shown24h - a.shown24h ||
      b.lastArticleAt.getTime() - a.lastArticleAt.getTime(),
  );
}
