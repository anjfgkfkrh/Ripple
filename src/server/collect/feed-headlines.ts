import { and, desc, gte, isNotNull, lt } from "drizzle-orm";
import type { Db } from "@/server/db/client";
import { feedHeadlines } from "@/server/db/schema";
import { effectivePublishedAt } from "./published";
import { urlKey } from "./url";

const HOUR_MS = 60 * 60 * 1000;
/** 제목을 두는 기간(10단계 결정: 3일) */
export const HEADLINE_KEEP_MS = 3 * 24 * HOUR_MS;
const TITLE_MAX_CHARS = 300;
const SUMMARY_MAX_CHARS = 300;

export type HeadlineItem = {
  title: string;
  publisher: string;
  publishedAt: Date | null;
  /** 원문 주소와 요약(11단계 결정: 새 토픽에 지난 기사를 걸 때 쓴다) */
  url?: string;
  summary?: string;
};

/** 같은 제목인지 볼 때 쓰는 열쇠: 공백을 모두 빼고 소문자로 */
export function headlineKey(title: string): string {
  return title.replace(/\s+/g, "").toLowerCase();
}

/**
 * 피드에서 받은 기사를 종목 연결과 상관없이 남긴다(토픽 갱신의 재료이자 지난 기사 걸기에 씀, 10·11단계).
 * 제목·언론사·피드·시각과 원문 주소·요약(앞 300자)을 둔다. 같은 제목은 처음 받은 것 하나만,
 * 보관 기간보다 오래된 기사는 넣지 않는다. 새로 넣은 수를 돌려준다.
 */
export function saveFeedHeadlines(
  db: Db,
  feedId: string,
  items: HeadlineItem[],
  seenAt: Date,
): number {
  const oldest = seenAt.getTime() - HEADLINE_KEEP_MS;
  const rows = items.flatMap((item) => {
    const title = item.title
      .replace(/\s+/g, " ")
      .trim()
      .slice(0, TITLE_MAX_CHARS);
    const publishedAt = effectivePublishedAt(item.publishedAt, seenAt);
    if (!title || publishedAt.getTime() < oldest) return [];
    const key = item.url ? urlKey(item.url) : null;
    const summary = item.summary
      ? item.summary.replace(/\s+/g, " ").trim().slice(0, SUMMARY_MAX_CHARS)
      : "";
    return [
      {
        titleKey: headlineKey(title),
        title,
        publisher: item.publisher,
        feed: feedId,
        publishedAt,
        seenAt,
        url: key && item.url ? item.url.trim() : null,
        urlKey: key,
        summary: summary || null,
      },
    ];
  });
  if (rows.length === 0) return 0;
  return db.transaction((tx) => {
    let added = 0;
    for (const row of rows) {
      added += tx
        .insert(feedHeadlines)
        .values(row)
        .onConflictDoNothing()
        .run().changes;
    }
    return added;
  });
}

/** 보관 기간이 지난 제목을 지운다. 지운 수를 돌려준다. */
export function pruneFeedHeadlines(db: Db, now: Date): number {
  return db
    .delete(feedHeadlines)
    .where(
      lt(feedHeadlines.publishedAt, new Date(now.getTime() - HEADLINE_KEEP_MS)),
    )
    .run().changes;
}

/** since 이후 제목을 최근순으로 limit개까지 */
export function recentFeedTitles(db: Db, since: Date, limit: number): string[] {
  return db
    .select({ title: feedHeadlines.title })
    .from(feedHeadlines)
    .where(gte(feedHeadlines.publishedAt, since))
    .orderBy(desc(feedHeadlines.publishedAt), desc(feedHeadlines.id))
    .limit(limit)
    .all()
    .map((row) => row.title);
}

export type StoredHeadline = typeof feedHeadlines.$inferSelect;

/** since 이후 제목 가운데 원문 주소가 있는 것(지난 기사를 다시 걸 때, 11단계) */
export function storedHeadlinesSince(db: Db, since: Date): StoredHeadline[] {
  return db
    .select()
    .from(feedHeadlines)
    .where(
      and(
        gte(feedHeadlines.publishedAt, since),
        isNotNull(feedHeadlines.urlKey),
      ),
    )
    .orderBy(desc(feedHeadlines.publishedAt), desc(feedHeadlines.id))
    .all();
}
