import { and, desc, eq, gt, inArray, isNull } from "drizzle-orm";
import type { Db } from "@/server/db/client";
import { collectionRuns, stocks } from "@/server/db/schema";
import { readSetting, SETTING_DEFAULTS } from "@/server/settings";
import { lastSeoulBoundary, nextSeoulBoundary } from "@/server/time";

/** 고를 수 있는 수집 간격(분) (SPEC FR-6) */
export const COLLECT_INTERVALS = [15, 30, 60, 180] as const;
/** 회차 전체가 실패하면 이만큼 뒤에 다시 한다(D-24). */
export const RETRY_AFTER_FAILURE_MS = 5 * 60 * 1000;
/** 예정 시각보다 이만큼 넘게 늦게 시작하면 '누락 보정'으로 기록한다(worker 꺼짐·노트북 절전). */
export const CATCH_UP_AFTER_MS = 5 * 60 * 1000;
/** 키워드 자동 재생성 시각(한국 시간, 2026-10-21 결정) */
export const KEYWORD_REGENERATE_HOUR = 6;

const MINUTE_MS = 60 * 1000;

export function readCollectInterval(db: Db): number {
  const value = readSetting(db, "collect.intervalMinutes");
  return (COLLECT_INTERVALS as readonly number[]).includes(value)
    ? value
    : SETTING_DEFAULTS["collect.intervalMinutes"];
}

export function readRegenerateEvery(db: Db): "daily" | "weekly" {
  return readSetting(db, "keywords.regenerateEvery") === "weekly"
    ? "weekly"
    : "daily";
}

export type CollectionPlan = {
  intervalMinutes: number;
  /** 등록한 종목이 없어 정기 수집을 쉰다(6단계 결정: 회차를 만들지 않음). */
  idle: boolean;
  /** 지금 도는 회차가 있다. */
  running: boolean;
  /** 다음 정기 수집 시각. 이미 지났으면 다음 확인 때 바로 시작한다. */
  nextAt: Date;
  /** 지난 회차가 모두 실패해 5분 뒤 다시 하는 중(D-24) */
  retrying: boolean;
  due: boolean;
  trigger: "scheduled" | "catch_up";
};

/**
 * 정기 수집 계획(D-31): 마지막으로 성공한 회차(일부 실패 포함, "지금 수집"도 셈) 시작 + 간격.
 * 그 뒤에 회차 전체가 실패했으면 실패한 회차 시작 + 5분과 비교해 늦은 쪽(D-24).
 * DB만 읽으므로 web 화면도 같은 함수로 다음 시각을 보여 준다.
 */
export function collectionPlan(db: Db, now: Date): CollectionPlan {
  const intervalMinutes = readCollectInterval(db);
  const idle = !db
    .select({ id: stocks.id })
    .from(stocks)
    .where(isNull(stocks.deletedAt))
    .limit(1)
    .get();
  const running = Boolean(
    db
      .select({ id: collectionRuns.id })
      .from(collectionRuns)
      .where(eq(collectionRuns.status, "running"))
      .limit(1)
      .get(),
  );
  const lastOk = db
    .select({ startedAt: collectionRuns.startedAt })
    .from(collectionRuns)
    .where(inArray(collectionRuns.status, ["success", "partial"]))
    .orderBy(desc(collectionRuns.startedAt))
    .limit(1)
    .get();
  const lastFailed = db
    .select({ startedAt: collectionRuns.startedAt })
    .from(collectionRuns)
    .where(
      and(
        eq(collectionRuns.status, "failed"),
        lastOk ? gt(collectionRuns.startedAt, lastOk.startedAt) : undefined,
      ),
    )
    .orderBy(desc(collectionRuns.startedAt))
    .limit(1)
    .get();

  const regular = lastOk
    ? lastOk.startedAt.getTime() + intervalMinutes * MINUTE_MS
    : 0;
  const retry = lastFailed
    ? lastFailed.startedAt.getTime() + RETRY_AFTER_FAILURE_MS
    : 0;
  // 한 번도 돈 적이 없으면 바로 시작한다.
  const next = lastOk || lastFailed ? Math.max(regular, retry) : now.getTime();
  return {
    intervalMinutes,
    idle,
    running,
    nextAt: new Date(next),
    retrying: Boolean(lastFailed) && retry >= regular,
    due: !idle && !running && now.getTime() >= next,
    trigger:
      now.getTime() - next > CATCH_UP_AFTER_MS ? "catch_up" : "scheduled",
  };
}

export type KeywordPlan = {
  every: "daily" | "weekly";
  /** 등록한 종목이 없다. */
  idle: boolean;
  /** 이번 주기(06:00)에 아직 다시 만들지 않았으면 지금, 했으면 다음 06:00 */
  nextAt: Date;
  due: boolean;
  /** 지금 다시 만들 종목: 이번 주기 시작 전에 만든 키워드를 가진 종목(만드는 중·대기 중은 뺀다) */
  stockIds: number[];
};

/**
 * 키워드 자동 재생성 계획(SPEC FR-3, 2026-10-21 결정): 매일 한국 시간 06:00(일주일 1회면 월요일 06:00).
 * 그 시각에 꺼져 있었으면 켜진 뒤 처음 확인할 때 한 번. 주기마다 한 번만 한다(실패해도 다음 주기에 다시).
 */
export function keywordPlan(db: Db, now: Date): KeywordPlan {
  const every = readRegenerateEvery(db);
  const weekly = every === "weekly";
  const boundary = lastSeoulBoundary(now, KEYWORD_REGENERATE_HOUR, weekly);
  const due =
    readSetting(db, "keywords.autoRegeneratedAt") < boundary.getTime();
  const active = db
    .select({
      id: stocks.id,
      status: stocks.keywordsStatus,
      generatedAt: stocks.keywordsGeneratedAt,
    })
    .from(stocks)
    .where(isNull(stocks.deletedAt))
    .all();
  return {
    every,
    idle: active.length === 0,
    nextAt: due ? now : nextSeoulBoundary(now, KEYWORD_REGENERATE_HOUR, weekly),
    due,
    stockIds: due
      ? active
          .filter(
            (s) =>
              s.status !== "pending" &&
              s.status !== "generating" &&
              (!s.generatedAt || s.generatedAt < boundary),
          )
          .map((s) => s.id)
      : [],
  };
}
