import { describe, expect, it } from "vitest";
import { RSS_FEEDS } from "@/server/collect/feeds";
import { LISTING_SOURCES } from "@/server/listings/sync";
import { readWorkerConfig } from "./config";

const TOKEN = "t".repeat(32);

describe("readWorkerConfig", () => {
  it("기본값: 127.0.0.1:4000, 공식 OpenRouter·목록·Finnhub·TypeSafe 주소, 기본 RSS 목록, 키 없음", () => {
    const config = readWorkerConfig({
      DATABASE_PATH: "./data/ripple.db",
      WORKER_API_TOKEN: TOKEN,
    });
    expect(config).toEqual({
      databasePath: "./data/ripple.db",
      host: "127.0.0.1",
      port: 4000,
      apiToken: TOKEN,
      openRouterApiKey: undefined,
      openRouterBaseUrl: "https://openrouter.ai/api/v1",
      listingSources: LISTING_SOURCES,
      finnhubApiKey: undefined,
      finnhubBaseUrl: "https://finnhub.io/api/v1",
      rssFeeds: RSS_FEEDS,
      typesafeApiKey: undefined,
      typesafeBaseUrl: "https://api.typesafe.ai",
    });
  });

  it("테스트용 원천 주소를 재정의할 수 있다", () => {
    const config = readWorkerConfig({
      DATABASE_PATH: "x.db",
      WORKER_API_TOKEN: TOKEN,
      OPENROUTER_API_KEY: " sk-or-1 ",
      OPENROUTER_BASE_URL: "http://127.0.0.1:4200/openrouter",
      LISTING_KIND_URL: "http://127.0.0.1:4200/kind",
    });
    expect(config.openRouterApiKey).toBe("sk-or-1");
    expect(config.openRouterBaseUrl).toBe("http://127.0.0.1:4200/openrouter");
    expect(config.listingSources.kind).toBe("http://127.0.0.1:4200/kind");
    expect(config.listingSources.nasdaqListed).toBe(
      LISTING_SOURCES.nasdaqListed,
    );
  });

  it("테스트용으로 RSS 목록과 Finnhub 주소를 바꿀 수 있다", () => {
    const feeds = [
      {
        id: "rss:mock:economy",
        publisher: "가짜경제",
        section: "경제",
        url: "http://127.0.0.1:4200/rss/economy.xml",
      },
    ];
    const config = readWorkerConfig({
      DATABASE_PATH: "x.db",
      WORKER_API_TOKEN: TOKEN,
      FINNHUB_API_KEY: " fh ",
      FINNHUB_BASE_URL: "http://127.0.0.1:4200/finnhub",
      RSS_FEEDS_JSON: JSON.stringify(feeds),
    });
    expect(config.finnhubApiKey).toBe("fh");
    expect(config.finnhubBaseUrl).toBe("http://127.0.0.1:4200/finnhub");
    expect(config.rssFeeds).toEqual(feeds);
  });

  it("TypeSafe 키와 테스트용 주소를 읽는다", () => {
    const config = readWorkerConfig({
      DATABASE_PATH: "x.db",
      WORKER_API_TOKEN: TOKEN,
      TYPESAFE_API_KEY: " ts-1 ",
      TYPESAFE_BASE_URL: "http://127.0.0.1:4200/typesafe",
    });
    expect(config.typesafeApiKey).toBe("ts-1");
    expect(config.typesafeBaseUrl).toBe("http://127.0.0.1:4200/typesafe");
    expect(
      readWorkerConfig({
        DATABASE_PATH: "x.db",
        WORKER_API_TOKEN: TOKEN,
        TYPESAFE_API_KEY: "  ",
      }).typesafeApiKey,
    ).toBeUndefined();
  });

  it("RSS_FEEDS_JSON이 잘못되면 시작하지 않는다", () => {
    const base = { DATABASE_PATH: "x.db", WORKER_API_TOKEN: TOKEN };
    expect(() => readWorkerConfig({ ...base, RSS_FEEDS_JSON: "{" })).toThrow(
      "RSS_FEEDS_JSON",
    );
    expect(() =>
      readWorkerConfig({ ...base, RSS_FEEDS_JSON: '[{"id":"x"}]' }),
    ).toThrow("RSS_FEEDS_JSON");
  });

  it("수집 목록은 id가 겹치지 않고 모두 https 주소다", () => {
    expect(new Set(RSS_FEEDS.map((f) => f.id)).size).toBe(RSS_FEEDS.length);
    expect(RSS_FEEDS.every((f) => f.url.startsWith("https://"))).toBe(true);
    expect(new Set(RSS_FEEDS.map((f) => f.publisher)).size).toBe(29);
  });

  it("worker 토큰이 없으면 시작하지 않는다", () => {
    expect(() => readWorkerConfig({ DATABASE_PATH: "x.db" })).toThrow(
      "WORKER_API_TOKEN",
    );
  });
});
