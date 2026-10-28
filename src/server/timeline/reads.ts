import { inArray } from "drizzle-orm";
import type { Db } from "@/server/db/client";
import { articleReads, articles } from "@/server/db/schema";

/** 한 번에 읽음으로 바꿀 수 있는 기사 수(한 카드에 묶이는 기사보다 넉넉하게) */
export const MAX_READ_BATCH = 200;

/**
 * 이 기사들을 읽음(read=true) 또는 안 읽음으로 저장한다(12단계).
 * 없는 기사 id는 건너뛰고, 이미 읽은 기사의 읽은 시각은 바꾸지 않는다. 실제로 바뀐 기사 수를 돌려준다.
 */
export function setArticlesRead(
  db: Db,
  articleIds: number[],
  read: boolean,
  now: Date = new Date(),
): number {
  const ids = [...new Set(articleIds)];
  if (ids.length === 0) return 0;
  return db.transaction((tx) => {
    if (!read) {
      return tx
        .delete(articleReads)
        .where(inArray(articleReads.articleId, ids))
        .run().changes;
    }
    const existing = tx
      .select({ id: articles.id })
      .from(articles)
      .where(inArray(articles.id, ids))
      .all();
    if (existing.length === 0) return 0;
    return tx
      .insert(articleReads)
      .values(existing.map((row) => ({ articleId: row.id, readAt: now })))
      .onConflictDoNothing()
      .run().changes;
  });
}

/** 이 기사들 가운데 읽은 기사의 id */
export function readArticleIds(db: Db, articleIds: number[]): Set<number> {
  if (articleIds.length === 0) return new Set();
  return new Set(
    db
      .select({ id: articleReads.articleId })
      .from(articleReads)
      .where(inArray(articleReads.articleId, articleIds))
      .all()
      .map((row) => row.id),
  );
}
