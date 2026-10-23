import { LoaderCircleIcon } from "lucide-react";
import { AutoRefresh } from "@/components/auto-refresh";
import { requireSession } from "@/server/auth/guard";
import { getWebDb } from "@/server/db/web";
import { readSetting } from "@/server/settings";
import { formatSeoulDateTime } from "@/server/time";
import { parseTopicTimes, topicPlan } from "@/server/topics/plan";
import { activeAutoTopics } from "@/server/topics/store";
import { next } from "./collection-panel";
import { RefreshTopicsButton } from "./refresh-topics-button";

/** 자동 토픽 갱신 상태(11단계): 마지막·다음 갱신, 실패 이유, "지금 갱신". 갱신 중이면 화면이 스스로 다시 읽는다. */
export async function TopicPanel() {
  await requireSession();
  const db = getWebDb();
  const now = new Date();
  const plan = topicPlan(db, now);
  const count = activeAutoTopics(db).length;
  const interval = readSetting(db, "topics.intervalHours");
  const times = parseTopicTimes(readSetting(db, "topics.times")).map(
    (t) =>
      `${String(t.hour).padStart(2, "0")}:${String(t.minute).padStart(2, "0")}`,
  );
  const rule = [
    ...(interval > 0 ? [`${interval}시간마다`] : []),
    ...(times.length > 0 ? [times.join("·")] : []),
  ].join(" + ");

  return (
    <div
      className="mt-3 space-y-3 rounded-lg border p-4"
      data-testid="topic-refresh"
    >
      <AutoRefresh active={plan.running} />
      <div className="flex flex-wrap items-start justify-between gap-3">
        <p className="text-sm text-muted-foreground">
          진행 중인 토픽 {count}개 · 마지막 갱신{" "}
          {plan.lastAt
            ? formatSeoulDateTime(plan.lastAt).slice(5, 16)
            : "아직 없음"}
        </p>
        <RefreshTopicsButton running={plan.running} />
      </div>
      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
        <dt className="text-muted-foreground">토픽 갱신</dt>
        <dd>
          {!plan.enabled
            ? "꺼짐(설정에서 간격이나 시각을 넣으면 켜짐)"
            : plan.running
              ? `${rule} · 지금 갱신 중`
              : `${rule} · ${next(plan.nextAt ?? now, now)}`}
          {plan.running && (
            <LoaderCircleIcon
              className="ml-1 inline size-3.5 animate-spin"
              aria-hidden
            />
          )}
        </dd>
      </dl>
      {plan.failure && !plan.running && (
        <p className="text-sm text-destructive" data-testid="topic-failure">
          {formatSeoulDateTime(plan.failure.at).slice(5, 16)} 갱신 실패:{" "}
          {plan.failure.error}
        </p>
      )}
    </div>
  );
}
