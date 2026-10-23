import { ChevronRightIcon, LoaderCircleIcon } from "lucide-react";
import { AutoRefresh } from "@/components/auto-refresh";
import { badgeVariants } from "@/components/ui/badge";
import { requireSession } from "@/server/auth/guard";
import {
  collectionTotals,
  groupFailures,
  latestRun,
  type CollectionRun,
} from "@/server/collect/read";
import type {
  EvaluationSummary,
  RunStatus,
  SourceResult,
} from "@/server/db/schema";
import { getWebDb } from "@/server/db/web";
import { evaluationTotals } from "@/server/evaluate/read";
import {
  collectionPlan,
  keywordPlan,
  type CollectionPlan,
  type KeywordPlan,
} from "@/server/schedule/plan";
import { formatSeoulDateTime } from "@/server/time";
import { CollectNowButton } from "./collect-now-button";

const STATUS_LABEL: Record<RunStatus, string> = {
  running: "수집 중",
  success: "완료",
  partial: "일부 실패",
  failed: "실패",
};

/** 최근 수집 회차와 저장·평가된 기사 수. 수집 중이면 화면이 스스로 다시 읽는다. */
export async function CollectionPanel() {
  await requireSession();
  const db = getWebDb();
  const run = latestRun(db);
  const totals = collectionTotals(db);
  const evaluated = evaluationTotals(db);
  const running = run?.status === "running";
  const now = new Date();

  return (
    <div
      className="mt-3 space-y-4 rounded-lg border p-4"
      data-testid="collection"
    >
      <AutoRefresh active={running} />
      <div className="flex flex-wrap items-start justify-between gap-3">
        <p className="text-sm text-muted-foreground">
          저장된 기사 {totals.articles}건 · 종목 연결 {totals.matches}건 ·{" "}
          {/* 좁은 화면에서 '(6점 이상 / 0건)'처럼 끊기지 않게 한 덩어리로 둔다. */}
          <span className="whitespace-nowrap">
            평가 {evaluated.evaluated}건(표시 기준 이상 {evaluated.relevant}건)
          </span>
        </p>
        <CollectNowButton running={running} />
      </div>
      <ScheduleView
        collect={collectionPlan(db, now)}
        keywords={keywordPlan(db, now)}
        now={now}
      />
      {run ? (
        <RunView run={run} />
      ) : (
        <p className="text-sm text-muted-foreground">
          아직 수집하지 않았습니다.
        </p>
      )}
    </div>
  );
}

function RunView({ run }: { run: CollectionRun }) {
  const seconds = run.finishedAt
    ? Math.max(
        0,
        Math.round((run.finishedAt.getTime() - run.startedAt.getTime()) / 1000),
      )
    : null;
  const failures = groupFailures(run.sources);
  const groups = groupSources(run.sources);

  return (
    <div className="space-y-3" data-testid="collection-run">
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm">
        <span
          className={badgeVariants({
            variant:
              run.status === "failed"
                ? "destructive"
                : run.status === "partial"
                  ? "outline"
                  : "secondary",
          })}
        >
          {run.status === "running" && (
            <LoaderCircleIcon className="animate-spin" aria-hidden />
          )}
          {STATUS_LABEL[run.status]}
        </span>
        <span className="text-muted-foreground">
          {formatSeoulDateTime(run.startedAt).slice(0, 16)} 시작
          {seconds !== null && ` · ${seconds}초`}
        </span>
      </div>
      {run.status !== "running" && (
        <p className="text-sm">
          새 기사 <strong>{run.articlesNew}</strong>건 · 새 연결{" "}
          <strong>{run.matchesNew}</strong>건
          {/* 점 앞은 줄바꿈 없는 공백이라 점이 다음 줄 맨 앞으로 가지 않는다. */}
          <span className="text-muted-foreground">{"\u00a0· "}</span>
          <span className="whitespace-nowrap text-muted-foreground">
            {formatSeoulDateTime(run.since).slice(0, 16)} 이후 기사
          </span>
        </p>
      )}
      {run.note && <p className="text-sm text-muted-foreground">{run.note}</p>}
      {run.evaluation && <EvaluationView evaluation={run.evaluation} />}
      {failures.length > 0 && (
        <ul
          className="space-y-1 text-sm text-destructive"
          aria-label="실패한 소스"
        >
          {failures.map(({ reason, labels }) => (
            <li key={reason}>
              {labels.length === 1
                ? labels[0]
                : `${labels[0]} 등 ${labels.length}곳`}
              : {reason}
            </li>
          ))}
        </ul>
      )}
      {groups.length > 0 && (
        <details className="group">
          <summary className="flex w-fit cursor-pointer list-none items-center gap-1 text-sm text-muted-foreground select-none [&::-webkit-details-marker]:hidden">
            <ChevronRightIcon
              className="size-3.5 transition-transform group-open:rotate-90"
              aria-hidden
            />
            출처별 결과 {groups.length}곳
          </summary>
          <ul
            className="mt-2 divide-y text-sm"
            data-testid="collection-sources"
          >
            {groups.map((g) => (
              <li
                key={g.name}
                className="flex flex-wrap justify-between gap-x-3 gap-y-0.5 py-1.5"
              >
                <span>{g.name}</span>
                <span className="text-muted-foreground tabular-nums">
                  받음 {g.fetched} · 새 기사 {g.saved} · 연결 {g.matched}
                  {g.errors > 0 && (
                    <span className="text-destructive"> · 실패 {g.errors}</span>
                  )}
                </span>
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}

/** 다음 정기 수집과 키워드 자동 재생성 시각(6단계 결정). 계산은 worker와 같은 함수로 한다. */
function ScheduleView({
  collect,
  keywords,
  now,
}: {
  collect: CollectionPlan;
  keywords: KeywordPlan;
  now: Date;
}) {
  return (
    <dl
      className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm"
      data-testid="collection-schedule"
    >
      <dt className="text-muted-foreground">정기 수집</dt>
      <dd>
        {collect.idle
          ? "등록한 종목이 없어 쉬는 중"
          : collect.running
            ? `${collect.intervalMinutes}분마다 · 지금 수집 중`
            : `${collect.intervalMinutes}분마다 · ${next(collect.nextAt, now)}`}
        {collect.retrying && !collect.running && !collect.idle && (
          <>
            {" "}
            <span className="whitespace-nowrap">(실패해 5분 뒤 다시)</span>
          </>
        )}
      </dd>
      <dt className="text-muted-foreground">키워드 다시 만들기</dt>
      <dd>
        {keywords.every === "weekly" ? "매주 월요일 06:00" : "매일 06:00"}
        {!keywords.idle && ` · ${next(keywords.nextAt, now)}`}
      </dd>
    </dl>
  );
}

/** 이미 지났으면 '곧 시작'(worker가 1분 안에 시작), 오늘이면 시각만, 아니면 날짜와 시각(한국 시간) */
export function next(at: Date, now: Date): string {
  if (at <= now) return "곧 시작";
  const target = formatSeoulDateTime(at);
  return `다음 ${
    target.slice(0, 10) === formatSeoulDateTime(now).slice(0, 10)
      ? target.slice(11, 16)
      : target.slice(5, 16)
  }`;
}

/** 이번 회차의 Jev 평가 결과(SPEC FR-5). 같은 소식의 앞선 평가를 쓴 수도 보인다(11단계). */
function EvaluationView({ evaluation: e }: { evaluation: EvaluationSummary }) {
  return (
    <div className="space-y-1 text-sm" data-testid="collection-evaluation">
      <p>
        평가 <strong>{e.evaluated}</strong>건
        {(e.copied ?? 0) > 0 && ` · 같은 소식 점수 재사용 ${e.copied}건`} · 표시
        기준 이상 <strong>{e.relevant}</strong>건
        {e.failed > 0 && (
          <span className="text-destructive"> · 실패 {e.failed}건</span>
        )}
        <span className="text-muted-foreground">
          {e.remaining > 0 && ` · 남김 ${e.remaining}건`} · $
          {e.costUsd.toFixed(5)}
        </span>
      </p>
      {e.note && <p className="text-muted-foreground">{e.note}</p>}
    </div>
  );
}

type SourceGroup = {
  name: string;
  fetched: number;
  saved: number;
  matched: number;
  errors: number;
};

/** 피드 결과를 출처(언론사·Finnhub)별로 묶는다. 새 기사가 많은 곳부터. */
function groupSources(sources: SourceResult[]): SourceGroup[] {
  const byName = new Map<string, SourceGroup>();
  for (const s of sources) {
    const name = s.group ?? s.label;
    const g = byName.get(name) ?? {
      name,
      fetched: 0,
      saved: 0,
      matched: 0,
      errors: 0,
    };
    g.fetched += s.fetched;
    g.saved += s.saved;
    g.matched += s.matched;
    if (s.error) g.errors += 1;
    byName.set(name, g);
  }
  return [...byName.values()].sort(
    (a, b) =>
      b.saved - a.saved ||
      b.matched - a.matched ||
      a.name.localeCompare(b.name, "ko"),
  );
}
