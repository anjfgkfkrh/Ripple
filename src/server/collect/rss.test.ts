import { describe, expect, it } from "vitest";
import { decodeEntities, parseFeed, parseFeedDate, toText } from "./rss";

const NOW = new Date("2026-10-20T10:00:00Z");

const rss = (items: string) =>
  `<?xml version="1.0" encoding="utf-8"?><rss version="2.0"><channel><title>t</title><link>https://example.com</link>${items}</channel></rss>`;

describe("parseFeed: RSS 2.0", () => {
  it("제목·링크·요약·발행 시각을 읽고 CDATA·HTML·엔티티를 정리한다", () => {
    const items = parseFeed(
      rss(`
        <item>
          <title><![CDATA[삼성전자, HBM <b>공급</b> 확대 &amp; 증설]]></title>
          <link>https://news.example.com/a/1?utm_source=rss</link>
          <description>&lt;p&gt;메모리 &amp;middot; 파운드리&lt;/p&gt;&lt;img src="x.jpg"&gt;</description>
          <pubDate>Tue, 20 Oct 2026 17:36:24 +0900</pubDate>
        </item>`),
      NOW,
    );
    expect(items).toEqual([
      {
        title: "삼성전자, HBM 공급 확대 & 증설",
        link: "https://news.example.com/a/1?utm_source=rss",
        summary: "메모리 · 파운드리",
        publishedAt: new Date("2026-10-20T08:36:24Z"),
      },
    ]);
  });

  it("link가 없으면 permalink guid를 쓰고, 제목이나 주소가 없으면 버린다", () => {
    const items = parseFeed(
      rss(`
        <item><title>guid만 있음</title><guid isPermaLink="true">https://e.com/g</guid></item>
        <item><title>주소 없음</title><guid>12345</guid></item>
        <item><title></title><link>https://e.com/no-title</link></item>`),
      NOW,
    );
    expect(items.map((i) => [i.title, i.link])).toEqual([
      ["guid만 있음", "https://e.com/g"],
    ]);
  });

  it("발행 시각이 없으면 null(한겨레처럼)", () => {
    const [item] = parseFeed(
      rss("<item><title>a</title><link>https://e.com/1</link></item>"),
      NOW,
    );
    expect(item.publishedAt).toBeNull();
  });

  it("content:encoded는 요약으로 쓰지 않는다(description만)", () => {
    const [item] = parseFeed(
      rss(
        "<item><title>a</title><link>https://e.com/1</link><content:encoded><![CDATA[<p>본문 전체</p>]]></content:encoded></item>",
      ),
      NOW,
    );
    expect(item.summary).toBe("");
  });
});

describe("parseFeed: Atom", () => {
  it("entry의 alternate 링크와 published를 읽는다", () => {
    const items = parseFeed(
      `<feed xmlns="http://www.w3.org/2005/Atom">
        <entry>
          <title type="html">Nvidia &amp; TSMC</title>
          <link rel="self" href="https://e.com/self"/>
          <link rel="alternate" href="https://e.com/a?x=1&amp;y=2"/>
          <summary>AI chips</summary>
          <published>2026-10-20T01:02:03Z</published>
        </entry>
      </feed>`,
      NOW,
    );
    expect(items).toEqual([
      {
        title: "Nvidia & TSMC",
        link: "https://e.com/a?x=1&y=2",
        summary: "AI chips",
        publishedAt: new Date("2026-10-20T01:02:03Z"),
      },
    ]);
  });
});

describe("parseFeedDate", () => {
  it.each([
    ["Tue, 20 Oct 2026 17:36:24 +0900", "2026-10-20T08:36:24Z"],
    ["Mon, 19 Oct 2026 23:50:00 GMT", "2026-10-19T23:50:00Z"],
    ["2026-10-20T09:00:00+09:00", "2026-10-20T00:00:00Z"],
    ["2026-10-20T09:00:00Z", "2026-10-20T09:00:00Z"],
    // 노컷뉴스: 월이 숫자
    ["Tue, 20 10 2026 17:36:24 +0900", "2026-10-20T08:36:24Z"],
    // 시간대 없음 → 한국 시간
    ["2026-10-20 17:36:24", "2026-10-20T08:36:24Z"],
    ["2026-10-20 17:36", "2026-10-20T08:36:00Z"],
  ])("%s", (raw, iso) => {
    expect(parseFeedDate(raw, NOW)?.toISOString()).toBe(
      new Date(iso).toISOString(),
    );
  });

  it("읽을 수 없거나 말이 안 되는 시각은 null", () => {
    expect(parseFeedDate("", NOW)).toBeNull();
    expect(parseFeedDate("어제", NOW)).toBeNull();
    expect(parseFeedDate("Mon, 01 Jan 0001 00:00:00 GMT", NOW)).toBeNull();
    expect(parseFeedDate("2026-10-23T00:00:00Z", NOW)).toBeNull();
  });
});

describe("toText·decodeEntities", () => {
  it("숫자·이름 엔티티를 풀고 모르는 엔티티는 그대로 둔다", () => {
    expect(decodeEntities("&#44032;&#xAC01; &hellip; &unknown;")).toBe(
      "가각 … &unknown;",
    );
    expect(toText("  a\n\n  b  ")).toBe("a b");
    expect(toText(null)).toBe("");
  });
});
