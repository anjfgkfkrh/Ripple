import { describe, expect, it } from "vitest";
import { urlKey } from "./url";

describe("urlKey (D-19 중복 판정)", () => {
  it("스킴·끝 슬래시·조각·추적 파라미터 차이는 같은 기사로 본다", () => {
    const keys = [
      "https://News.Example.com/a/1/",
      "http://news.example.com/a/1",
      "https://news.example.com/a/1#comments",
      "https://news.example.com/a/1?utm_source=rss&utm_medium=feed",
    ].map(urlKey);
    expect(new Set(keys)).toEqual(new Set(["news.example.com/a/1"]));
  });

  it("기사를 가리키는 쿼리는 남기고 순서를 맞춘다(SBS처럼 추적용 plink·cooper는 뺀다)", () => {
    expect(
      urlKey(
        "https://news.sbs.co.kr/news/endPage.do?news_id=N1&plink=RSSLINK&cooper=RSSREADER",
      ),
    ).toBe("news.sbs.co.kr/news/endPage.do?news_id=N1");
    expect(urlKey("https://e.com/v?b=2&a=1")).toBe(
      urlKey("https://e.com/v?a=1&b=2"),
    );
    expect(urlKey("https://e.com/v?id=1")).not.toBe(
      urlKey("https://e.com/v?id=2"),
    );
  });

  it("포트는 남기고, http(s)가 아니거나 주소가 아니면 null", () => {
    expect(urlKey("http://127.0.0.1:4200/x")).toBe("127.0.0.1:4200/x");
    expect(urlKey("ftp://e.com/x")).toBeNull();
    expect(urlKey("기사")).toBeNull();
  });
});
