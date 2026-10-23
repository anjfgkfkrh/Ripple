import type { Db } from "@/server/db/client";
import { storedHeadlinesSince } from "./feed-headlines";
import { createMatcher } from "./match";
import { loadMatchStocks, saveItems } from "./run";

/**
 * 보관한 제목(원문 주소·요약 포함, 3일)으로 since 이후 기사를 주어진 항목에만 다시 건다(11단계 결정).
 * 새로 잡힌 토픽의 항목과, 토픽 가지를 붙인 종목에 지난 24시간 기사까지 걸 때 쓴다.
 * 이미 걸린 기사–항목은 그대로 두고, 평가는 다음 수집 회차에서 한다.
 */
export function linkStoredHeadlines(
  db: Db,
  stockIds: number[],
  since: Date,
  now: Date,
): { articles: number; matches: number } {
  const total = { articles: 0, matches: 0 };
  if (stockIds.length === 0) return total;
  const wanted = new Set(stockIds);
  const watched = loadMatchStocks(db).filter((stock) => wanted.has(stock.id));
  if (watched.length === 0) return total;
  const matcher = createMatcher(watched);
  for (const headline of storedHeadlinesSince(db, since)) {
    if (!headline.url || !headline.urlKey) continue;
    const summary = headline.summary ?? "";
    const matches = matcher(`${headline.title} ${summary}`);
    if (matches.length === 0) continue;
    const saved = saveItems(
      db,
      headline.feed,
      [
        {
          item: {
            url: headline.url,
            title: headline.title,
            summary,
            publisher: headline.publisher,
            publishedAt: headline.publishedAt,
          },
          key: headline.urlKey,
          publishedAt: headline.publishedAt,
          matches,
        },
      ],
      now,
    );
    total.articles += saved.articles;
    total.matches += saved.matches;
  }
  return total;
}
