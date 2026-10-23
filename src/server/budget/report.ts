import { desc, eq, gte, sql } from "drizzle-orm";
import type { Db } from "@/server/db/client";
import { stocks, topics, usageLogs } from "@/server/db/schema";
import { readSetting } from "@/server/settings";
import { startOfSeoulMonth } from "@/server/time";
import { monthSpendUsd } from "./usage";

/** 이만큼 쓰면 미리 알린다(D-07, 2026-10-21 결정). */
export const BUDGET_WARN_RATIO = 0.8;

/** ok: 여유, warn: 80% 넘음, over: 한도에 닿아 AI 호출을 멈춤(D-07) */
export type BudgetLevel = "ok" | "warn" | "over";

export function budgetLevel(spentUsd: number, limitUsd: number): BudgetLevel {
  if (spentUsd >= limitUsd) return "over";
  if (spentUsd >= limitUsd * BUDGET_WARN_RATIO) return "warn";
  return "ok";
}

/** 사용량 기록의 용도(purpose)를 화면 이름으로 */
export const PURPOSE_LABEL: Record<string, string> = {
  stock_match: "종목 찾기",
  keyword_generation: "키워드 만들기",
  market_issues: "시장 이슈 정리",
  topic_refresh: "토픽 갱신",
  keyword_rating: "키워드·토픽 평가",
  article_evaluation: "기사 평가",
  keyword_generation_check: "품질 확인(개발 중 직접 호출)",
  market_issues_check: "품질 확인(개발 중 직접 호출)",
  topic_refresh_check: "품질 확인(개발 중 직접 호출)",
  keyword_rating_check: "품질 확인(개발 중 직접 호출)",
  article_evaluation_check: "품질 확인(개발 중 직접 호출)",
  model_compare: "품질 확인(개발 중 직접 호출)",
};

export type UsageRow = {
  provider: "openrouter" | "typesafe";
  purpose: string;
  label: string;
  calls: number;
  costUsd: number;
  /** 이번 달 전체 비용 중 비율 0~1 */
  share: number;
};

/**
 * 항목별 비용의 묶음(11단계 결정): 종목·시장·주제(사용자 주제·자동 토픽)·공용(토픽 갱신·시장 이슈·종목 찾기 등).
 * unknown은 항목마다 쓰는 용도인데 항목을 남기지 않은 기록(11단계 전)이다.
 */
export const ITEM_GROUPS = [
  "stock",
  "market",
  "topic",
  "shared",
  "unknown",
] as const;
export type ItemGroup = (typeof ITEM_GROUPS)[number];
export const ITEM_GROUP_LABEL: Record<ItemGroup, string> = {
  stock: "종목",
  market: "시장",
  topic: "주제·토픽",
  shared: "공용",
  unknown: "항목 기록 없음(11단계 전)",
};

export type ItemUsageRow = {
  /** 화면 key: 종목·시장은 항목 id, 사용자 주제는 토픽 id, 자동 토픽은 하나로 묶음 */
  key: string;
  group: ItemGroup;
  name: string;
  /** 지운 항목 */
  deleted: boolean;
  calls: number;
  costUsd: number;
  share: number;
};

export type GroupUsageRow = {
  group: ItemGroup;
  label: string;
  calls: number;
  costUsd: number;
  share: number;
};

/** 항목마다 쓰는 용도: 항목이 없으면 11단계 전 기록이다. */
const PER_ITEM_PURPOSES = new Set([
  "keyword_generation",
  "keyword_rating",
  "article_evaluation",
]);

/**
 * 이번 달(한국 시간) 제공자·용도별 호출 수와 비용, 한도(SPEC FR-8, 2026-10-21 결정).
 * 11단계: 기능별 비율과, 항목별(종목·시장·주제·공용) 비용·비율도 낸다.
 */
export function monthUsageReport(
  db: Db,
  now: Date = new Date(),
): {
  rows: UsageRow[];
  groups: GroupUsageRow[];
  items: ItemUsageRow[];
  totalUsd: number;
  limitUsd: number;
  level: BudgetLevel;
} {
  const rows = db
    .select({
      provider: usageLogs.provider,
      purpose: usageLogs.purpose,
      calls: sql<number>`count(*)`,
      costUsd: sql<number>`coalesce(sum(${usageLogs.costUsd}), 0)`,
    })
    .from(usageLogs)
    .where(gte(usageLogs.at, startOfSeoulMonth(now)))
    .groupBy(usageLogs.provider, usageLogs.purpose)
    .orderBy(desc(sql`sum(${usageLogs.costUsd})`))
    .all();
  // 화면 이름이 같은 용도(개발 중 품질 확인 여러 가지)는 제공자마다 한 줄로 합친다.
  const merged = new Map<string, UsageRow>();
  for (const row of rows) {
    const label = PURPOSE_LABEL[row.purpose] ?? row.purpose;
    const key = `${row.provider}:${label}`;
    const prev = merged.get(key);
    merged.set(key, {
      provider: row.provider,
      purpose: prev?.purpose ?? row.purpose,
      label,
      calls: (prev?.calls ?? 0) + Number(row.calls),
      costUsd: (prev?.costUsd ?? 0) + Number(row.costUsd),
      share: 0,
    });
  }
  const totalUsd = monthSpendUsd(db, now);
  const limitUsd = readSetting(db, "budget.monthlyUsd");
  const share = (costUsd: number) => (totalUsd > 0 ? costUsd / totalUsd : 0);
  const { groups, items } = usageByItem(db, now);
  return {
    rows: [...merged.values()]
      .map((row) => ({ ...row, share: share(row.costUsd) }))
      .sort((a, b) => b.costUsd - a.costUsd),
    groups: groups.map((row) => ({ ...row, share: share(row.costUsd) })),
    items: items.map((row) => ({ ...row, share: share(row.costUsd) })),
    totalUsd,
    limitUsd,
    level: budgetLevel(totalUsd, limitUsd),
  };
}

/** 이번 달 비용을 항목별로 모은다. 같은 주제의 증시별 항목은 하나로, 자동 토픽은 모두 한 줄로 묶는다. */
function usageByItem(
  db: Db,
  now: Date,
): {
  groups: Omit<GroupUsageRow, "share">[];
  items: Omit<ItemUsageRow, "share">[];
} {
  const rows = db
    .select({
      stockId: usageLogs.stockId,
      purpose: usageLogs.purpose,
      kind: stocks.kind,
      name: stocks.name,
      nameKo: stocks.nameKo,
      deletedAt: stocks.deletedAt,
      topicId: stocks.topicId,
      topicSource: topics.source,
      topicName: topics.name,
      topicDeletedAt: topics.deletedAt,
      calls: sql<number>`count(*)`,
      costUsd: sql<number>`coalesce(sum(${usageLogs.costUsd}), 0)`,
    })
    .from(usageLogs)
    .leftJoin(stocks, eq(stocks.id, usageLogs.stockId))
    .leftJoin(topics, eq(topics.id, stocks.topicId))
    .where(gte(usageLogs.at, startOfSeoulMonth(now)))
    .groupBy(usageLogs.stockId, usageLogs.purpose)
    .all();

  const items = new Map<string, Omit<ItemUsageRow, "share">>();
  const autoTopics = new Set<number>();
  for (const row of rows) {
    let item: Omit<ItemUsageRow, "share" | "calls" | "costUsd">;
    if (row.stockId === null || row.kind === null) {
      const group: ItemGroup =
        row.stockId === null && !PER_ITEM_PURPOSES.has(row.purpose)
          ? "shared"
          : "unknown";
      item = {
        key: group,
        group,
        name:
          group === "shared"
            ? "공용(토픽 갱신·시장 이슈·종목 찾기 등)"
            : ITEM_GROUP_LABEL.unknown,
        deleted: false,
      };
    } else if (row.kind === "topic" && row.topicSource === "auto") {
      if (row.topicId !== null) autoTopics.add(row.topicId);
      item = {
        key: "auto-topics",
        group: "topic",
        name: "자동 토픽",
        deleted: false,
      };
    } else if (row.kind === "topic") {
      item = {
        key: `topic-${row.topicId}`,
        group: "topic",
        name: row.topicName ?? row.nameKo ?? row.name ?? "",
        deleted: row.topicDeletedAt !== null,
      };
    } else {
      item = {
        key: `item-${row.stockId}`,
        group: row.kind,
        name: row.nameKo ?? row.name ?? "",
        deleted: row.deletedAt !== null,
      };
    }
    const prev = items.get(item.key);
    items.set(item.key, {
      ...item,
      calls: (prev?.calls ?? 0) + Number(row.calls),
      costUsd: (prev?.costUsd ?? 0) + Number(row.costUsd),
    });
  }
  const auto = items.get("auto-topics");
  if (auto) auto.name = `자동 토픽(${autoTopics.size}개)`;

  const groups = ITEM_GROUPS.flatMap((group) => {
    const mine = [...items.values()].filter((item) => item.group === group);
    if (mine.length === 0) return [];
    return [
      {
        group,
        label: ITEM_GROUP_LABEL[group],
        calls: mine.reduce((sum, item) => sum + item.calls, 0),
        costUsd: mine.reduce((sum, item) => sum + item.costUsd, 0),
      },
    ];
  });
  return {
    groups,
    items: [...items.values()].sort(
      (a, b) => b.costUsd - a.costUsd || a.name.localeCompare(b.name, "ko"),
    ),
  };
}
