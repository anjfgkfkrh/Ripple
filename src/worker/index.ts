import { JEV_QUESTION_LANGUAGE } from "@/server/ai/decider/questions";
import { createTypeSafeDecider } from "@/server/ai/decider/typesafe";
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
import { startScheduler } from "@/server/schedule/scheduler";
import { readWorkerConfig } from "./config";
import { createWorkerServer } from "./server";

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
  if (!config.typesafeApiKey) {
    log("TYPESAFE_API_KEY가 없어 기사 평가(Jev)는 하지 않습니다.");
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
    decider: config.typesafeApiKey
      ? createTypeSafeDecider({
          apiKey: config.typesafeApiKey,
          baseUrl: config.typesafeBaseUrl,
          language: JEV_QUESTION_LANGUAGE,
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

  // 대기 중이었거나 만들다 멈춘(재시작 등) 키워드 생성을 이어서 한다.
  const resumed = stocksAwaitingKeywords(db);
  if (resumed.length > 0)
    log(`키워드 생성 이어서 하기: ${resumed.length}개 종목`);
  for (const stockId of resumed) keywordQueue.enqueue(stockId);

  // 정기 수집·키워드 자동 재생성·공식 목록 확인(6단계). 시작하자마자 한 번 확인해 놓친 일을 한다.
  if (!config.schedulerJobsEnabled) {
    log("SCHEDULER_DISABLED: 정기 수집과 키워드 자동 재생성을 하지 않습니다.");
  }
  const scheduler = startScheduler({
    db,
    collector,
    keywordQueue,
    syncListings: createListingSync(db, config.listingSources),
    jobsEnabled: config.schedulerJobsEnabled,
  });

  let stopping = false;
  const shutdown = (signal: string) => {
    if (stopping) return;
    stopping = true;
    log(`${signal} 수신, 종료합니다.`);
    scheduler.stop();
    server.close();
    server.closeAllConnections();
    db.$client.close();
    process.exit(0);
  };
  process.on("SIGTERM", () => shutdown("SIGTERM"));
  process.on("SIGINT", () => shutdown("SIGINT"));
}

/** 공식 종목 목록이 오래됐으면 받는다(D-12). 겹쳐 돌지 않고, 던지지 않는다. */
function createListingSync(
  db: Db,
  sources: ListingSources,
): () => Promise<void> {
  let running = false;
  return async () => {
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
    } catch (error) {
      logError("공식 종목 목록 확인 실패", error);
    } finally {
      running = false;
    }
  };
}

main().catch((error: unknown) => {
  logError("worker 시작 실패", error);
  process.exit(1);
});
