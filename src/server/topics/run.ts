import { eq } from "drizzle-orm";
import { DeciderError, type Decider } from "@/server/ai/decider/types";
import { LlmError, type LlmClient } from "@/server/ai/llm/types";
import {
  BudgetExceededError,
  generateWithBudget,
  rateWithBudget,
} from "@/server/budget/usage";
import { linkStoredHeadlines } from "@/server/collect/backfill";
import { recentFeedTitles } from "@/server/collect/feed-headlines";
import type { Db } from "@/server/db/client";
import { topics } from "@/server/db/schema";
import { judgeItem } from "@/server/keywords/generate";
import { log, logError } from "@/server/log";
import { readSetting, writeSetting } from "@/server/settings";
import {
  buildTopicRefreshMessages,
  cleanTopicRefresh,
  topicRefreshSchema,
  type RefreshedTopic,
} from "./refresh";
import {
  activeAutoTopics,
  activeStocks,
  attachedTopicsByStock,
  attachTopicToStock,
  detachTopicFromStock,
  setAutoTopicTerms,
  syncTopicLenses,
  topicLenses,
  type Topic,
} from "./store";

const HOUR_MS = 60 * 60 * 1000;
/** 한 번에 보는 제목의 기간: 지난 갱신 이후, 길어도 24시간 */
const MAX_WINDOW_MS = 24 * HOUR_MS;
/** 새 제목이 이보다 적으면(밤·꺼져 있던 때) 토픽을 바꾸지 않는다. */
export const MIN_REFRESH_TITLES = 20;
/**
 * 한 번에 넣는 제목 수의 상한. 기본 모델(Haiku 5.5)은 입력이 10만 토큰을 넘으면 단가가 5배가 되므로
 * 그보다 넉넉히 아래에 둔다(2026-10-22 결정, 실측 1,500개에 약 6만 토큰).
 */
export const MAX_REFRESH_TITLES = 1500;
/** 새 토픽·붙인 가지에 거는 지난 기사 기간(11단계 결정) */
const BACKFILL_MS = 24 * HOUR_MS;
/** 2026-10-22 실측: 제목 1,500개에 약 11초 */
const REFRESH_TIMEOUT_MS = 120_000;

export type TopicRefreshSummary = {
  titles: number;
  topics: number;
  continued: number;
  created: number;
  ended: number;
  attached: number;
  detached: number;
  /** 지난 24시간 기사로 새로 건 연결 수 */
  linked: number;
  /** 바꾸지 않은 이유(제목 부족 등) */
  skipped?: string;
  /** 관련 종목을 Jev로 평가하지 못한 이유(키 없음·예산 등) */
  ratingStopped?: string;
};

/**
 * 자동 토픽을 갱신한다(11단계, 토픽 우선). 지난 갱신 이후(길어도 24시간)의 제목과 이전 토픽 목록으로 AI가
 * 지금 시장을 움직이는 토픽을 정하고(이어지는 것·새것·식은 것), 토픽마다 영향을 받을 내 종목 후보를 낸다.
 * Jev가 토픽×종목 쌍의 관련도·영향도를 매겨 기준을 넘으면 그 종목에 토픽 가지를 붙이고, 아니면 뗀다.
 * 새 토픽 항목과 새로 붙인 가지에는 보관한 제목으로 지난 24시간 기사를 건다(평가는 다음 수집 회차).
 * AI 호출이 실패하면 던진다(스케줄러가 잠시 뒤 다시 한다).
 */
export async function refreshTopics(deps: {
  db: Db;
  llm: LlmClient;
  decider: Decider | null;
  now?: () => Date;
}): Promise<TopicRefreshSummary> {
  const { db } = deps;
  const now = deps.now ?? (() => new Date());
  const at = now();
  const summary: TopicRefreshSummary = {
    titles: 0,
    topics: 0,
    continued: 0,
    created: 0,
    ended: 0,
    attached: 0,
    detached: 0,
    linked: 0,
  };
  const refreshedAt = readSetting(db, "topics.refreshedAt");
  const since = new Date(
    Math.max(at.getTime() - MAX_WINDOW_MS, refreshedAt || 0),
  );
  const titles = recentFeedTitles(db, since, MAX_REFRESH_TITLES);
  summary.titles = titles.length;
  const previous = activeAutoTopics(db);
  if (titles.length < MIN_REFRESH_TITLES) {
    writeSetting(db, "topics.refreshedAt", at.getTime());
    summary.topics = previous.length;
    summary.skipped = `새 제목이 ${titles.length}개라 토픽을 그대로 둡니다(${MIN_REFRESH_TITLES}개 미만).`;
    return summary;
  }

  const stocksNow = activeStocks(db);
  const itemNames = stocksNow.map((stock) => stock.nameKo ?? stock.name);
  const { value } = await generateWithBudget(
    db,
    deps.llm,
    {
      model: readSetting(db, "llm.model"),
      messages: buildTopicRefreshMessages({
        previous: previous.map((topic) => ({
          id: String(topic.id),
          name: topic.name,
          summary: topic.summary ?? "",
          terms: topic.terms,
        })),
        titles,
        items: itemNames,
      }),
      schema: topicRefreshSchema,
      schemaName: "topic_refresh",
      maxTokens: 6000,
      timeoutMs: REFRESH_TIMEOUT_MS,
    },
    { purpose: "topic_refresh", now },
  );
  const refreshed = continueTopics(
    cleanTopicRefresh(
      value,
      new Set(previous.map((topic) => String(topic.id))),
      itemNames,
    ),
    previous,
  );

  // 토픽을 고치거나 만들고, 이번에 빠진 것은 끝낸다(끝난 토픽은 새 기사를 걸지 않는다).
  const newLenses: number[] = [];
  const current: { topic: Topic; refreshed: RefreshedTopic }[] = [];
  db.transaction(() => {
    const kept = new Set<number>();
    for (const item of refreshed) {
      const id = item.id === null ? null : Number(item.id);
      let topic: Topic;
      if (id !== null) {
        topic = db
          .update(topics)
          .set({
            name: item.name,
            summary: item.summary,
            terms: item.terms,
            lastSeenAt: at,
          })
          .where(eq(topics.id, id))
          .returning()
          .get();
        kept.add(id);
        summary.continued += 1;
        for (const lens of topicLenses(db, id)) {
          if (lens.deletedAt === null)
            setAutoTopicTerms(db, lens.id, topic, at);
        }
      } else {
        topic = db
          .insert(topics)
          .values({
            source: "auto",
            name: item.name,
            summary: item.summary,
            terms: item.terms,
            createdAt: at,
            lastSeenAt: at,
          })
          .returning()
          .get();
        summary.created += 1;
      }
      newLenses.push(...syncTopicLenses(db, topic, at));
      current.push({ topic, refreshed: item });
    }
    for (const topic of previous) {
      if (kept.has(topic.id)) continue;
      db.update(topics)
        .set({ status: "ended", endedAt: at })
        .where(eq(topics.id, topic.id))
        .run();
      summary.ended += 1;
    }
  });
  summary.topics = current.length;

  // 토픽×종목: AI가 낸 관련 종목 후보를 Jev로 평가해 붙이거나 뗀다.
  const attachedBefore = attachedTopicsByStock(db);
  const wanted = new Map<number, Set<number>>();
  const newlyAttached = new Set<number>();
  const minRelevance = readSetting(db, "keywords.minRelevance");
  const minMagnitude = readSetting(db, "keywords.minMagnitude");
  let ratingStopped: string | undefined = deps.decider
    ? undefined
    : "TYPESAFE_API_KEY가 없습니다.";
  for (const { topic, refreshed: item } of current) {
    for (const relation of item.related) {
      const stock = stocksNow.find(
        (row) => (row.nameKo ?? row.name) === relation.item,
      );
      if (!stock) continue;
      if (ratingStopped || !deps.decider) {
        // 평가할 수 없으면 이미 붙어 있던 가지는 그대로 둔다.
        if (attachedBefore.get(stock.id)?.has(topic.id)) {
          addTo(wanted, stock.id, topic.id);
        }
        continue;
      }
      try {
        const rating = await rateWithBudget(
          db,
          deps.decider,
          {
            stock: judgeItem(stock, null),
            subject: {
              type: "topic",
              text: topic.name,
              summary: topic.summary ?? undefined,
              terms: topic.terms,
              reason: relation.reason || null,
            },
          },
          { purpose: "keyword_rating", now, stockId: stock.id },
        );
        if (
          rating.relevance >= minRelevance &&
          rating.magnitude >= minMagnitude
        ) {
          addTo(wanted, stock.id, topic.id);
          const fresh = attachTopicToStock(
            db,
            stock,
            topic,
            {
              reason: relation.reason,
              relevance: rating.relevance,
              magnitude: rating.magnitude,
            },
            at,
          );
          if (fresh) {
            summary.attached += 1;
            newlyAttached.add(stock.id);
          }
        }
      } catch (error) {
        if (
          error instanceof BudgetExceededError ||
          (error instanceof DeciderError && error.fatal)
        ) {
          ratingStopped = error.message;
          if (attachedBefore.get(stock.id)?.has(topic.id)) {
            addTo(wanted, stock.id, topic.id);
          }
          continue;
        }
        logError(
          `토픽 평가 실패: ${topic.name} × ${relation.item}`,
          error instanceof Error ? error.message : error,
        );
        if (attachedBefore.get(stock.id)?.has(topic.id)) {
          addTo(wanted, stock.id, topic.id);
        }
      }
    }
  }
  if (ratingStopped) summary.ratingStopped = ratingStopped;
  for (const [stockId, topicIds] of attachedBefore) {
    for (const topicId of topicIds) {
      if (wanted.get(stockId)?.has(topicId)) continue;
      if (detachTopicFromStock(db, stockId, topicId)) summary.detached += 1;
    }
  }

  // 새 토픽 항목과 새로 붙인 가지에 지난 24시간 기사를 건다(11단계 결정).
  const linkTargets = [...newLenses, ...newlyAttached];
  if (linkTargets.length > 0) {
    try {
      summary.linked = linkStoredHeadlines(
        db,
        linkTargets,
        new Date(at.getTime() - BACKFILL_MS),
        at,
      ).matches;
    } catch (error) {
      logError("지난 기사 걸기 실패", error);
    }
  }
  writeSetting(db, "topics.refreshedAt", at.getTime());
  return summary;
}

/**
 * AI가 이전 토픽의 id를 돌려주지 않을 때가 있어(2026-10-22 실측: 이어진 9개를 모두 새 토픽으로 냄),
 * id가 없는 토픽은 이름이 같거나 검색어가 하나라도 같은 이전 토픽으로 잇는다.
 */
export function continueTopics(
  refreshed: RefreshedTopic[],
  previous: Pick<Topic, "id" | "name" | "terms">[],
): RefreshedTopic[] {
  const key = (text: string) => text.replace(/\s+/g, "").toLowerCase();
  const used = new Set(
    refreshed.flatMap((topic) => (topic.id === null ? [] : [topic.id])),
  );
  return refreshed.map((topic) => {
    if (topic.id !== null) return topic;
    const terms = new Set(topic.terms.map(key));
    const match = previous.find(
      (prev) =>
        !used.has(String(prev.id)) &&
        (key(prev.name) === key(topic.name) ||
          prev.terms.some((term) => terms.has(key(term)))),
    );
    if (!match) return topic;
    used.add(String(match.id));
    return { ...topic, id: String(match.id) };
  });
}

function addTo(map: Map<number, Set<number>>, key: number, value: number) {
  const set = map.get(key) ?? new Set<number>();
  set.add(value);
  map.set(key, set);
}

export type TopicRefresher = {
  /** 갱신을 시작한다. 이미 하고 있으면 그 갱신을 기다린다. 던지지 않는다(성공하면 true). */
  refresh(): Promise<boolean>;
  running(): boolean;
};

/** 정기 갱신(스케줄러)과 "지금 갱신"(상태 화면)이 겹쳐 돌지 않게 하나로 묶는다. */
export function createTopicRefresher(deps: {
  db: Db;
  llm: LlmClient;
  decider: Decider | null;
  now?: () => Date;
}): TopicRefresher {
  let current: Promise<boolean> | null = null;
  return {
    refresh() {
      current ??= refreshTopicsAndLog(deps).finally(() => {
        current = null;
      });
      return current;
    },
    running: () => current !== null,
  };
}

/**
 * 스케줄러·"지금 갱신"용: 실패해도 던지지 않고 로그만 남긴다. 성공하면 요약 한 줄.
 * 시작·실패 시각과 이유를 설정에 남겨 상태 화면이 보이고, 실패하면 30분 뒤 다시 한다(topicPlan).
 */
export async function refreshTopicsAndLog(deps: {
  db: Db;
  llm: LlmClient;
  decider: Decider | null;
  now?: () => Date;
}): Promise<boolean> {
  const started = Date.now();
  writeSetting(
    deps.db,
    "topics.startedAt",
    (deps.now?.() ?? new Date()).getTime(),
  );
  try {
    const s = await refreshTopics(deps);
    const seconds = ((Date.now() - started) / 1000).toFixed(1);
    if (s.skipped) {
      log(`토픽 갱신 건너뜀: ${s.skipped}`);
    } else {
      log(
        `토픽 갱신: 제목 ${s.titles}개 → 토픽 ${s.topics}개(이어짐 ${s.continued}, 새로 ${s.created}, 끝남 ${s.ended}), 종목에 붙임 ${s.attached}·뗌 ${s.detached}, 지난 기사 ${s.linked}건 연결${s.ratingStopped ? `, 관련 종목 평가 못 함: ${s.ratingStopped}` : ""}, ${seconds}초`,
      );
    }
    return true;
  } catch (error) {
    logError("토픽 갱신 실패", error);
    writeSetting(
      deps.db,
      "topics.failedAt",
      (deps.now?.() ?? new Date()).getTime(),
    );
    writeSetting(deps.db, "topics.error", describeRefreshError(error));
    return false;
  }
}

/** 상태 화면에 보일 실패 이유 */
export function describeRefreshError(error: unknown): string {
  if (error instanceof BudgetExceededError) {
    return "이번 달 AI 예산을 모두 썼습니다.";
  }
  if (error instanceof LlmError) {
    switch (error.kind) {
      case "no_api_key":
        return "OpenRouter API 키가 없습니다.";
      case "payment_required":
        return "OpenRouter 크레딧이 부족하거나 키의 사용 한도에 걸렸습니다.";
      case "rate_limited":
        return "OpenRouter 요청이 많아 잠시 막혔습니다.";
      case "timeout":
        return "AI 응답이 늦어 멈췄습니다.";
      case "invalid_output":
        return "AI 응답 형식이 맞지 않았습니다.";
      case "upstream":
        return "OpenRouter 오류가 났습니다.";
    }
  }
  return "알 수 없는 오류가 났습니다(worker 로그 확인).";
}
