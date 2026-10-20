import { createOpenRouterClient } from "@/server/ai/llm/openrouter";
import { createFinnhubClient } from "@/server/collect/finnhub";
import { createHttpClient } from "@/server/collect/http";
import { createRobotsChecker } from "@/server/collect/robots";
import { closeStaleRuns } from "@/server/collect/run";
import { createCollectionRunner } from "@/server/collect/runner";
import { enableWal, openDb, type Db } from "@/server/db/client";
import { stocksAwaitingKeywords } from "@/server/keywords/generate";
import { createKeywordQueue } from "@/server/keywords/queue";
import {
  syncListingsIfStale,
  type ListingSources,
} from "@/server/listings/sync";
import { log, logError } from "@/server/log";
import { readWorkerConfig } from "./config";
import { createWorkerServer } from "./server";

/**
 * 공식 종목 목록이 오래됐는지 확인하는 간격. 실제로 받는 것은 목록이 없거나 7일이 지났을 때뿐이다(D-12).
 * 처음 받기에 실패해도(네트워크 등) 30분 안에 다시 시도한다.
 */
const LISTING_CHECK_INTERVAL_MS = 30 * 60 * 1000;

async function main(): Promise<void> {
  const config = readWorkerConfig();

  const db = openDb(config.databasePath, "readwrite");
  enableWal(db);
  log(`DB 연결: ${config.databasePath} (WAL, 쓰기 가능)`);

  if (!config.openRouterApiKey) {
    log("OPENROUTER_API_KEY가 없어 AI 기능(종목 찾기 등)은 쓸 수 없습니다.");
  }
  const llm = createOpenRouterClient({
    apiKey: config.openRouterApiKey,
    baseUrl: config.openRouterBaseUrl,
  });

  const keywordQueue = createKeywordQueue({ db, llm });

  const stale = closeStaleRuns(db);
  if (stale > 0) log(`도중에 멈춘 수집 회차 ${stale}개를 실패로 정리했습니다.`);
  if (!config.finnhubApiKey) {
    log("FINNHUB_API_KEY가 없어 미국 뉴스(Finnhub)는 수집하지 않습니다.");
  }
  const http = createHttpClient();
  const collector = createCollectionRunner({
    db,
    http,
    robots: createRobotsChecker(http),
    feeds: config.rssFeeds,
    finnhub: config.finnhubApiKey
      ? createFinnhubClient({
          http,
          apiKey: config.finnhubApiKey,
          baseUrl: config.finnhubBaseUrl,
        })
      : null,
  });

  const server = createWorkerServer({
    db,
    apiToken: config.apiToken,
    llm,
    keywordQueue,
    collector,
  });
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(config.port, config.host, () => resolve());
  });
  log(`내부 API 대기 중: http://${config.host}:${config.port}`);

  // 6단계 스케줄러가 생기면 그쪽으로 옮긴다.
  const listingTimer = startListingSync(db, config.listingSources);

  // 대기 중이었거나 만들다 멈춘(재시작 등) 키워드 생성을 이어서 한다.
  const resumed = stocksAwaitingKeywords(db);
  if (resumed.length > 0)
    log(`키워드 생성 이어서 하기: ${resumed.length}개 종목`);
  for (const stockId of resumed) keywordQueue.enqueue(stockId);

  let stopping = false;
  const shutdown = (signal: string) => {
    if (stopping) return;
    stopping = true;
    log(`${signal} 수신, 종료합니다.`);
    clearInterval(listingTimer);
    server.close();
    server.closeAllConnections();
    db.$client.close();
    process.exit(0);
  };
  process.on("SIGTERM", () => shutdown("SIGTERM"));
  process.on("SIGINT", () => shutdown("SIGINT"));
}

function startListingSync(db: Db, sources: ListingSources): NodeJS.Timeout {
  let running = false;
  const run = async () => {
    if (running) return;
    running = true;
    try {
      for (const result of await syncListingsIfStale(db, { sources })) {
        if (result.ok) {
          log(`${result.market} 공식 종목 목록 갱신: ${result.count}개`);
        } else {
          logError(
            `${result.market} 공식 종목 목록 갱신 실패(이전 목록 유지)`,
            result.error,
          );
        }
      }
    } finally {
      running = false;
    }
  };
  void run();
  return setInterval(() => void run(), LISTING_CHECK_INTERVAL_MS);
}

main().catch((error: unknown) => {
  logError("worker 시작 실패", error);
  process.exit(1);
});
