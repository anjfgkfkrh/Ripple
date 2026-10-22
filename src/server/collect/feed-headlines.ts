import { desc, gte, lt } from "drizzle-orm";
import type { Db } from "@/server/db/client";
import { feedHeadlines } from "@/server/db/schema";

const HOUR_MS = 60 * 60 * 1000;
/** 제목을 두는 기간(10단계 결정: 3일) */
export const HEADLINE_KEEP_MS = 3 * 24 * HOUR_MS;
const TITLE_MAX_CHARS = 300;

export type HeadlineItem = {
  title: string;
  publisher: string;
  publishedAt: Date | null;
};

/** 같은 제목인지 볼 때 쓰는 열쇠: 공백을 모두 빼고 소문자로 */
export function headlineKey(title: string): string {
  return title.replace(/\s+/g, "").toLowerCase();
}

/**
 * 피드에서 받은 기사 제목을 종목 연결과 상관없이 남긴다(10단계, 시장 이슈 정리의 재료).
 * 같은 제목은 처음 받은 것 하나만, 보관 기간보다 오래된 기사는 넣지 않는다. 새로 넣은 수를 돌려준다.
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
    const publishedAt = item.publishedAt ?? seenAt;
    if (!title || publishedAt.getTime() < oldest) return [];
    return [
      {
        titleKey: headlineKey(title),
        title,
        publisher: item.publisher,
        feed: feedId,
        publishedAt,
        seenAt,
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
