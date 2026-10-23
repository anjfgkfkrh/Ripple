import type { Db } from "@/server/db/client";
import { readSetting } from "@/server/settings";
import { lastSeoulClock, nextSeoulClock } from "@/server/time";

const HOUR_MS = 60 * 60 * 1000;
/** 갱신 시각 형식: 한국 시간 "HH:MM" */
export const TOPIC_TIME = /^([01]\d|2[0-3]):([0-5]\d)$/;

export function parseTopicTimes(
  times: string[],
): { hour: number; minute: number }[] {
  return times.flatMap((time) => {
    const match = TOPIC_TIME.exec(time.trim());
    return match ? [{ hour: Number(match[1]), minute: Number(match[2]) }] : [];
  });
}

/** 갱신이 실패하면(AI 오류·예산 등) 이만큼 뒤에 다시 한다. */
export const TOPIC_RETRY_MS = 30 * 60 * 1000;
/** 시작하고 이만큼 지나도 끝난 기록이 없으면(worker가 도중에 꺼짐) 갱신 중으로 보지 않는다. */
export const TOPIC_STALE_MS = 10 * 60 * 1000;

export type TopicPlan = {
  enabled: boolean;
  due: boolean;
  /** 다음 갱신 예정 시각(꺼져 있으면 null). 지금 해야 하면 now */
  nextAt: Date | null;
  /** 마지막 갱신 시각(없으면 null) */
  lastAt: Date | null;
  /** 지금 갱신하는 중 */
  running: boolean;
  /** 마지막 시도가 실패했으면 그 시각과 이유(그 뒤에 성공했으면 null) */
  failure: { at: Date; error: string } | null;
};

/**
 * 자동 토픽 갱신 계획(11단계 결정): 사용자가 정한 간격(시간, 기본 3)과 특정 시각(한국 시간) 가운데
 * 먼저 오는 때 한다. 처음이면 바로 한다. 놓친 시각은 켜질 때 한 번만 한다.
 * 실패했으면 30분 뒤에 다시 한다. DB만 읽으므로 상태 화면도 같은 함수로 보여 준다.
 */
export function topicPlan(db: Db, now: Date): TopicPlan {
  const intervalHours = readSetting(db, "topics.intervalHours");
  const times = parseTopicTimes(readSetting(db, "topics.times"));
  const refreshedAt = readSetting(db, "topics.refreshedAt");
  const startedAt = readSetting(db, "topics.startedAt");
  const failedAt = readSetting(db, "topics.failedAt");
  const lastAt = refreshedAt > 0 ? new Date(refreshedAt) : null;
  const running =
    startedAt > Math.max(refreshedAt, failedAt) &&
    now.getTime() - startedAt < TOPIC_STALE_MS;
  const failure =
    failedAt > refreshedAt
      ? { at: new Date(failedAt), error: readSetting(db, "topics.error") }
      : null;
  const base = { lastAt, running, failure };
  if (intervalHours <= 0 && times.length === 0) {
    return { enabled: false, due: false, nextAt: null, ...base };
  }
  const intervalDue =
    intervalHours > 0 && now.getTime() - refreshedAt >= intervalHours * HOUR_MS;
  const lastClock = Math.max(
    0,
    ...times.map((t) => lastSeoulClock(now, t.hour, t.minute).getTime()),
  );
  const clockDue = times.length > 0 && refreshedAt < lastClock;
  if (intervalDue || clockDue) {
    const retryAt = failure ? failedAt + TOPIC_RETRY_MS : 0;
    if (now.getTime() < retryAt) {
      return { enabled: true, due: false, nextAt: new Date(retryAt), ...base };
    }
    return { enabled: true, due: !running, nextAt: now, ...base };
  }
  const candidates = [
    ...(intervalHours > 0 ? [refreshedAt + intervalHours * HOUR_MS] : []),
    ...times.map((t) => nextSeoulClock(now, t.hour, t.minute).getTime()),
  ];
  return {
    enabled: true,
    due: false,
    nextAt: new Date(Math.min(...candidates)),
    ...base,
  };
}
