import * as z from "zod";
import { OPENROUTER_BASE_URL } from "@/server/ai/llm/openrouter";
import { readWorkerApiToken } from "@/server/auth/worker-token";
import { RSS_FEEDS, type RssFeed } from "@/server/collect/feeds";
import { FINNHUB_BASE_URL } from "@/server/collect/finnhub";
import { readPort, requireEnv } from "@/server/env";
import { LISTING_SOURCES, type ListingSources } from "@/server/listings/sync";

type Env = Record<string, string | undefined>;

export type WorkerConfig = {
  databasePath: string;
  host: string;
  port: number;
  apiToken: string;
  /** 없으면 worker는 뜨지만 AI 기능(종목 찾기 등)은 오류를 돌려준다. */
  openRouterApiKey: string | undefined;
  openRouterBaseUrl: string;
  listingSources: ListingSources;
  /** 없으면 미국 뉴스(Finnhub)는 건너뛴다. */
  finnhubApiKey: string | undefined;
  finnhubBaseUrl: string;
  rssFeeds: RssFeed[];
};

const rssFeedsOverride = z
  .array(
    z.object({
      id: z.string().min(1),
      publisher: z.string().min(1),
      section: z.string().min(1),
      url: z.url(),
    }),
  )
  .min(1);

/**
 * 원천 주소 재정의(OPENROUTER_BASE_URL, LISTING_*_URL, FINNHUB_BASE_URL, RSS_FEEDS_JSON)는
 * 테스트에서 가짜 서버를 쓰기 위한 것이다.
 */
export function readWorkerConfig(env: Env = process.env): WorkerConfig {
  return {
    databasePath: requireEnv("DATABASE_PATH", env),
    // 로컬 실행 때는 밖에서 접근하지 못하게 127.0.0.1, Docker에서는 compose가 0.0.0.0을 준다.
    host: env.WORKER_HOST?.trim() || "127.0.0.1",
    port: readPort("WORKER_PORT", 4000, env),
    apiToken: readWorkerApiToken(env),
    openRouterApiKey: env.OPENROUTER_API_KEY?.trim() || undefined,
    openRouterBaseUrl: env.OPENROUTER_BASE_URL?.trim() || OPENROUTER_BASE_URL,
    listingSources: {
      kind: env.LISTING_KIND_URL?.trim() || LISTING_SOURCES.kind,
      nasdaqListed:
        env.LISTING_NASDAQ_URL?.trim() || LISTING_SOURCES.nasdaqListed,
      otherListed: env.LISTING_OTHER_URL?.trim() || LISTING_SOURCES.otherListed,
    },
    finnhubApiKey: env.FINNHUB_API_KEY?.trim() || undefined,
    finnhubBaseUrl: env.FINNHUB_BASE_URL?.trim() || FINNHUB_BASE_URL,
    rssFeeds: readRssFeeds(env.RSS_FEEDS_JSON),
  };
}

function readRssFeeds(json: string | undefined): RssFeed[] {
  if (!json?.trim()) return RSS_FEEDS;
  let value: unknown;
  try {
    value = JSON.parse(json);
  } catch {
    throw new Error("RSS_FEEDS_JSON이 올바른 JSON이 아닙니다.");
  }
  const parsed = rssFeedsOverride.safeParse(value);
  if (!parsed.success) {
    throw new Error(
      "RSS_FEEDS_JSON 형식이 다릅니다([{id, publisher, section, url}]).",
    );
  }
  return parsed.data;
}
