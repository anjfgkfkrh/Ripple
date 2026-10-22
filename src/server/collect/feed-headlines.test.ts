import { asc } from "drizzle-orm";
import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import { openDb, type Db } from "../db/client";
import { feedHeadlines } from "../db/schema";
import {
  HEADLINE_KEEP_MS,
  headlineKey,
  pruneFeedHeadlines,
  recentFeedTitles,
  saveFeedHeadlines,
} from "./feed-headlines";

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

const NOW = new Date("2026-10-22T01:00:00Z");
const HOUR = 60 * 60 * 1000;
const ago = (hours: number) => new Date(NOW.getTime() - hours * HOUR);

function rows(db: Db) {
  return db.select().from(feedHeadlines).orderBy(asc(feedHeadlines.id)).all();
}

describe("saveFeedHeadlines (10단계)", () => {
  it("제목·언론사·피드·시각만 남기고, 같은 제목(공백·대소문자 무시)은 하나", () => {
    const db = testDb();
    const added = saveFeedHeadlines(
      db,
      "rss:sbs:international",
      [
        {
          title: " 트럼프,  중동 순방 ",
          publisher: "SBS",
          publishedAt: ago(1),
        },
        { title: "트럼프, 중동 순방", publisher: "SBS", publishedAt: ago(2) },
        { title: "Fed Holds Rates", publisher: "SBS", publishedAt: ago(1) },
        { title: "fed holds rates", publisher: "SBS", publishedAt: ago(1) },
        { title: "   ", publisher: "SBS", publishedAt: ago(1) },
      ],
      NOW,
    );
    expect(added).toBe(2);
    expect(rows(db)).toEqual([
      {
        id: 1,
        titleKey: "트럼프,중동순방",
        title: "트럼프, 중동 순방",
        publisher: "SBS",
        feed: "rss:sbs:international",
        publishedAt: ago(1),
        seenAt: NOW,
      },
      expect.objectContaining({ title: "Fed Holds Rates" }),
    ]);

    // 다음 회차에 같은 제목이 또 와도 늘지 않는다.
    expect(
      saveFeedHeadlines(
        db,
        "rss:other:all",
        [
          {
            title: "트럼프, 중동 순방",
            publisher: "다른 곳",
            publishedAt: ago(0),
          },
        ],
        NOW,
      ),
    ).toBe(0);
    expect(rows(db)).toHaveLength(2);
  });

  it("발행 시각이 없으면 받은 시각, 3일보다 오래된 기사는 넣지 않고, 긴 제목은 자른다", () => {
    const db = testDb();
    saveFeedHeadlines(
      db,
      "rss:test:all",
      [
        { title: "시각 없음", publisher: "p", publishedAt: null },
        { title: "오래된 기사", publisher: "p", publishedAt: ago(73) },
        { title: "가".repeat(400), publisher: "p", publishedAt: ago(1) },
      ],
      NOW,
    );
    expect(rows(db).map((row) => [row.title.length, row.publishedAt])).toEqual([
      [5, NOW],
      [300, ago(1)],
    ]);
  });

  it("headlineKey는 공백을 모두 빼고 소문자로", () => {
    expect(headlineKey(" A b\tC ")).toBe("abc");
  });
});

describe("pruneFeedHeadlines·recentFeedTitles", () => {
  it("3일이 지난 제목을 지우고, 기간 안 제목을 최근순으로 limit개까지 준다", () => {
    const db = testDb();
    const at = new Date(NOW.getTime() - HEADLINE_KEEP_MS);
    saveFeedHeadlines(
      db,
      "rss:test:all",
      [
        { title: "가장 최근", publisher: "p", publishedAt: ago(1) },
        { title: "두 번째", publisher: "p", publishedAt: ago(2) },
        { title: "어제", publisher: "p", publishedAt: ago(25) },
        { title: "사흘 전", publisher: "p", publishedAt: ago(71) },
      ],
      NOW,
    );
    // 받을 때는 3일 안이었지만 시간이 지나 3일을 넘긴 제목
    saveFeedHeadlines(
      db,
      "rss:test:all",
      [
        {
          title: "지난 제목",
          publisher: "p",
          publishedAt: new Date(at.getTime() - HOUR),
        },
      ],
      new Date(at.getTime() - HOUR),
    );

    expect(pruneFeedHeadlines(db, NOW)).toBe(1);
    expect(recentFeedTitles(db, ago(24), 10)).toEqual(["가장 최근", "두 번째"]);
    expect(recentFeedTitles(db, ago(100), 3)).toEqual([
      "가장 최근",
      "두 번째",
      "어제",
    ]);
  });
});
