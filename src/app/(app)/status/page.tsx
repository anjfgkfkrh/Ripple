import { Suspense, type ReactNode } from "react";
import { requireSession } from "@/server/auth/guard";
import { getWebDb } from "@/server/db/web";
import { readSetting } from "@/server/settings";
import { getSystemStatus } from "@/server/status";
import { fetchTunnelStatus, type TunnelStatus } from "@/server/tunnel";
import { formatSeoulDateTime } from "@/server/time";
import { CollectionPanel } from "./collection-panel";

// 시스템 상태와 기사 수집 확인 화면. 7단계에서 홈(타임라인)에서 옮겼다(D-28·D-29 결정 전까지 임시).
export default function StatusPage() {
  return (
    <main className="mx-auto w-full max-w-xl px-4 py-8 sm:py-12">
      <h1 className="text-xl font-semibold">시스템 상태</h1>
      <p className="mt-1 text-sm text-muted-foreground">
        시스템과 기사 수집 상태를 확인합니다.
      </p>
      <Suspense
        fallback={
          <p className="mt-6 text-sm text-muted-foreground">확인 중…</p>
        }
      >
        <SystemStatusView />
      </Suspense>
      <section className="mt-10">
        <h2 className="text-base font-semibold">기사 수집</h2>
        <p className="mt-1 text-sm text-muted-foreground">
          언론사 RSS와 Finnhub(미국 종목)에서 등록한 종목에 걸린 기사를 정해진
          간격마다 모으고 Jev로 관련도(0~10)와 호재·악재를 평가합니다.
        </p>
        <Suspense
          fallback={
            <p className="mt-3 text-sm text-muted-foreground">불러오는 중…</p>
          }
        >
          <CollectionPanel />
        </Suspense>
      </section>
    </main>
  );
}

async function SystemStatusView() {
  // 데이터에 닿기 전에 로그인을 한 번 더 확인한다. 쿠키를 읽으므로 요청 시점에 실행된다.
  await requireSession();
  const [{ database, worker, checkedAt }, tunnel] = await Promise.all([
    getSystemStatus({ getDb: getWebDb, workerUrl: process.env.WORKER_URL }),
    fetchTunnelStatus(process.env.TUNNEL_METRICS_URL),
  ]);
  const db = getWebDb();
  const lastBackupAt = readSetting(db, "backup.lastAt");
  const lastBackupFile = readSetting(db, "backup.lastFile");

  return (
    <dl className="mt-6 divide-y rounded-lg border">
      <StatusRow
        label="DB (web: 읽기 전용)"
        testId="db-status"
        ok={database.ok}
      >
        {database.ok
          ? `journal_mode ${database.journalMode} · settings ${database.settingsCount}건`
          : database.reason}
      </StatusRow>
      <StatusRow label="worker 내부 API" testId="worker-status" ok={worker.ok}>
        {worker.ok
          ? worker.listings
            ? `응답함 · 공식 종목 목록 한국 ${worker.listings.KR}개, 미국 ${worker.listings.US}개`
            : "응답함"
          : worker.reason}
      </StatusRow>
      <InfoRow label="외부 접속(임시 주소)" testId="tunnel-status">
        <TunnelView tunnel={tunnel} />
      </InfoRow>
      <InfoRow label="백업(매일 04:00)" testId="backup-status">
        {lastBackupAt > 0
          ? `마지막 ${formatSeoulDateTime(new Date(lastBackupAt)).slice(5, 16)} · ${lastBackupFile} · 최근 7개 보관`
          : "아직 없습니다. Docker로 실행하면 매일 04:00에 backups 폴더에 남깁니다."}
      </InfoRow>
      <div className="flex flex-col gap-1 p-4 sm:flex-row sm:justify-between">
        <dt className="text-sm text-muted-foreground">확인 시각 (한국 시간)</dt>
        <dd className="text-sm tabular-nums">
          {formatSeoulDateTime(checkedAt)}
        </dd>
      </div>
    </dl>
  );
}

/** 휴대폰에서 접속할 Cloudflare 임시 주소(9단계). 재시작하면 바뀐다. */
function TunnelView({ tunnel }: { tunnel: TunnelStatus }) {
  switch (tunnel.state) {
    case "ready":
      return (
        <span className="flex flex-col gap-1 sm:items-end">
          <a
            href={tunnel.url}
            target="_blank"
            rel="noopener noreferrer"
            className="font-medium break-all text-foreground underline underline-offset-2"
          >
            {tunnel.url}
          </a>
          <span>
            휴대폰 브라우저에 이 주소를 넣으세요. Docker를 다시 켜면 주소가
            바뀝니다.
          </span>
        </span>
      );
    case "pending":
      return "주소를 받는 중입니다. 잠시 뒤 새로 고침하세요.";
    case "off":
      return "터널을 쓰지 않습니다(이 컴퓨터에서만 접속).";
    case "error":
      return `터널에 닿지 못했습니다: ${tunnel.reason}`;
  }
}

function InfoRow(props: {
  label: string;
  testId: string;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1 p-4 sm:flex-row sm:items-start sm:justify-between sm:gap-4">
      <dt className="shrink-0 text-sm text-muted-foreground">{props.label}</dt>
      <dd
        className="text-sm break-all text-muted-foreground sm:text-right"
        data-testid={props.testId}
      >
        {props.children}
      </dd>
    </div>
  );
}

function StatusRow(props: {
  label: string;
  testId: string;
  ok: boolean;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1 p-4 sm:flex-row sm:items-start sm:justify-between sm:gap-4">
      <dt className="text-sm text-muted-foreground">{props.label}</dt>
      <dd
        className="flex flex-col gap-1 text-sm sm:items-end"
        data-testid={props.testId}
      >
        <span
          className={
            props.ok
              ? "w-fit rounded bg-green-100 px-2 py-0.5 font-medium text-green-800 dark:bg-green-900/40 dark:text-green-300"
              : "w-fit rounded bg-destructive/10 px-2 py-0.5 font-medium text-destructive"
          }
        >
          {props.ok ? "정상" : "오류"}
        </span>
        <span className="break-all text-muted-foreground sm:text-right">
          {props.children}
        </span>
      </dd>
    </div>
  );
}
