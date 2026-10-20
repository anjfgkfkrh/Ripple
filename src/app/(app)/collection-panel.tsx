import { ChevronRightIcon, LoaderCircleIcon } from "lucide-react";
import { AutoRefresh } from "@/components/auto-refresh";
import { badgeVariants } from "@/components/ui/badge";
import { requireSession } from "@/server/auth/guard";
import {
  collectionTotals,
  latestRun,
  type CollectionRun,
} from "@/server/collect/read";
import type { RunStatus, SourceResult } from "@/server/db/schema";
import { getWebDb } from "@/server/db/web";
import { formatSeoulDateTime } from "@/server/time";
import { CollectNowButton } from "./collect-now-button";

const STATUS_LABEL: Record<RunStatus, string> = {
  running: "수집 중",
  success: "완료",
  partial: "일부 실패",
  failed: "실패",
};

/** 최근 수집 회차와 저장된 기사 수. 수집 중이면 화면이 스스로 다시 읽는다. */
export async function CollectionPanel() {
  await requireSession();
  const db = getWebDb();
  const run = latestRun(db);
  const totals = collectionTotals(db);
  const running = run?.status === "running";

  return (
    <div
      className="mt-3 space-y-4 rounded-lg border p-4"
      data-testid="collection"
    >
      <AutoRefresh active={running} />
      <div className="flex flex-wrap items-start justify-between gap-3">
        <p className="text-sm text-muted-foreground">
          저장된 기사 {totals.articles}건 · 종목 연결 {totals.matches}건
        </p>
        <CollectNowButton running={running} />
      </div>
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
  const failed = run.sources.filter((s) => s.error);
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
          <span className="text-muted-foreground">
            {" "}
            · {formatSeoulDateTime(run.since).slice(0, 16)} 이후 기사
          </span>
        </p>
      )}
      {run.note && <p className="text-sm text-muted-foreground">{run.note}</p>}
      {failed.length > 0 && (
        <ul
          className="space-y-1 text-sm text-destructive"
          aria-label="실패한 소스"
        >
          {failed.map((s) => (
            <li key={s.id}>
              {s.label}: {s.error}
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
