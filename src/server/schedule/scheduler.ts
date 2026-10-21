import type { CollectionRunner } from "@/server/collect/runner";
import type { Db } from "@/server/db/client";
import type { KeywordQueue } from "@/server/keywords/queue";
import { log, logError } from "@/server/log";
import { writeSetting } from "@/server/settings";
import { collectionPlan, keywordPlan } from "./plan";

/** 할 일을 확인하는 간격(D-31) */
export const SCHEDULER_TICK_MS = 60 * 1000;
/**
 * 공식 종목 목록이 오래됐는지 확인하는 간격. 실제로 받는 것은 목록이 없거나 7일이 지났을 때뿐이다(D-12).
 * 처음 받기에 실패해도(네트워크 등) 30분 안에 다시 시도한다.
 */
export const LISTING_CHECK_INTERVAL_MS = 30 * 60 * 1000;

const TRIGGER_LABEL = {
  scheduled: "정기",
  catch_up: "누락 보정",
} as const;

export type Scheduler = {
  /** 할 일을 한 번 확인한다(시작할 때와 1분마다). 던지지 않는다. */
  tick(): void;
  stop(): void;
};

/**
 * worker의 주기 작업(SPEC FR-6, D-31). 라이브러리 없이 1분마다 DB를 보고 할 일을 시작한다.
 * 시계(DB의 마지막 실행 시각)를 비교하므로 worker를 다시 켰을 때와 노트북이 절전에서 깨어났을 때
 * 놓친 일을 같은 방식으로 한 번 한다. 수집과 키워드 재생성은 서로 기다리지 않는다(2026-10-21 결정).
 */
export function startScheduler(deps: {
  db: Db;
  collector: CollectionRunner;
  keywordQueue: KeywordQueue;
  /** 공식 종목 목록이 오래됐으면 받는다. 던지지 않아야 한다. */
  syncListings: () => Promise<void>;
  /** false면 정기 수집·키워드 자동 재생성을 하지 않는다(E2E용). 목록 확인은 한다. */
  jobsEnabled?: boolean;
  now?: () => Date;
  tickMs?: number;
}): Scheduler {
  const now = deps.now ?? (() => new Date());
  const jobsEnabled = deps.jobsEnabled ?? true;
  let lastListingCheck = -Infinity;

  const tick = () => {
    const at = now();
    if (jobsEnabled) {
      try {
        const plan = collectionPlan(deps.db, at);
        if (plan.due) {
          const runId = deps.collector.start(plan.trigger);
          if (runId !== null) {
            const label = plan.retrying
              ? "실패 뒤 다시"
              : TRIGGER_LABEL[plan.trigger];
            log(`수집 시작: 회차 ${runId}(${label})`);
          }
        }
      } catch (error) {
        logError("정기 수집 확인 실패", error);
      }
      try {
        const plan = keywordPlan(deps.db, at);
        if (plan.due) {
          for (const stockId of plan.stockIds)
            deps.keywordQueue.enqueue(stockId);
          writeSetting(deps.db, "keywords.autoRegeneratedAt", at.getTime());
          log(
            `키워드 자동 재생성(${plan.every === "weekly" ? "매주 월요일" : "매일"} 06:00): ${plan.stockIds.length}개 종목`,
          );
        }
      } catch (error) {
        logError("키워드 자동 재생성 확인 실패", error);
      }
    }
    if (at.getTime() - lastListingCheck >= LISTING_CHECK_INTERVAL_MS) {
      lastListingCheck = at.getTime();
      void deps.syncListings();
    }
  };

  tick();
  const timer = setInterval(tick, deps.tickMs ?? SCHEDULER_TICK_MS);
  return {
    tick,
    stop: () => clearInterval(timer),
  };
}
