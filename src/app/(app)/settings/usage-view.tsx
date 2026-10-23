import { cn } from "cn";
import { ChevronRightIcon } from "lucide-react";
import type { ReactNode } from "react";
import type {
  BudgetLevel,
  GroupUsageRow,
  ItemUsageRow,
  UsageRow,
} from "@/server/budget/report";

const PROVIDER_LABEL = { openrouter: "OpenRouter", typesafe: "TypeSafe(Jev)" };

const BAR_CLASS: Record<BudgetLevel, string> = {
  ok: "bg-primary",
  warn: "bg-amber-500",
  over: "bg-destructive",
};

/**
 * 이번 달 AI 사용량: 한도 대비 막대(2026-10-21 결정), 기능별·항목별(종목·시장·주제·공용) 호출 수와 비용·비율(11단계)
 */
export function UsageView({
  report,
}: {
  report: {
    rows: UsageRow[];
    groups: GroupUsageRow[];
    items: ItemUsageRow[];
    totalUsd: number;
    limitUsd: number;
    level: BudgetLevel;
  };
}) {
  const percent =
    report.limitUsd > 0
      ? Math.min(100, (report.totalUsd / report.limitUsd) * 100)
      : 100;
  return (
    <section className="mt-10 space-y-3" data-testid="usage">
      <h2 className="text-base font-semibold">이번 달 AI 사용량</h2>
      <div>
        <p className="text-sm">
          <strong className="tabular-nums">
            ${report.totalUsd.toFixed(4)}
          </strong>
          <span className="text-muted-foreground">
            {" "}
            / 한도 ${report.limitUsd} ({Math.round(percent)}%)
          </span>
        </p>
        <div
          className="mt-2 h-2 w-full overflow-hidden rounded-full bg-muted"
          role="progressbar"
          aria-label="이번 달 예산 사용"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(percent)}
        >
          <div
            className={cn("h-full rounded-full", BAR_CLASS[report.level])}
            style={{ width: `${percent}%` }}
          />
        </div>
      </div>
      {report.rows.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          이번 달에는 아직 AI를 부르지 않았습니다.
        </p>
      ) : (
        <>
          <UsageTable title="기능별" head="기능" testId="usage-features">
            {report.rows.map((row) => (
              <UsageLine
                key={`${row.provider}:${row.purpose}`}
                calls={row.calls}
                costUsd={row.costUsd}
                share={row.share}
              >
                {row.label}
                <span className="text-xs text-muted-foreground">
                  {" "}
                  · {PROVIDER_LABEL[row.provider]}
                </span>
              </UsageLine>
            ))}
          </UsageTable>
          <UsageTable title="항목별" head="묶음" testId="usage-groups">
            {report.groups.map((row) => (
              <UsageLine
                key={row.group}
                calls={row.calls}
                costUsd={row.costUsd}
                share={row.share}
              >
                {row.label}
              </UsageLine>
            ))}
          </UsageTable>
          <details className="group">
            <summary className="flex w-fit cursor-pointer list-none items-center gap-1 text-sm text-muted-foreground select-none [&::-webkit-details-marker]:hidden">
              <ChevronRightIcon
                className="size-3.5 transition-transform group-open:rotate-90"
                aria-hidden
              />
              항목 하나씩 보기({report.items.length}개)
            </summary>
            <UsageTable head="항목" testId="usage-items">
              {report.items.map((row) => (
                <UsageLine
                  key={row.key}
                  calls={row.calls}
                  costUsd={row.costUsd}
                  share={row.share}
                >
                  {row.name}
                  <span className="text-xs text-muted-foreground">
                    {" "}
                    · {GROUP_SHORT[row.group]}
                    {row.deleted && " · 삭제함"}
                  </span>
                </UsageLine>
              ))}
            </UsageTable>
          </details>
          <p className="text-xs text-muted-foreground">
            비율은 이번 달 전체 비용 대비입니다. 토픽–종목 평가는 그 종목에,
            주제·토픽 기사 평가는 그 주제에 셉니다.
          </p>
        </>
      )}
    </section>
  );
}

const GROUP_SHORT: Record<ItemUsageRow["group"], string> = {
  stock: "종목",
  market: "시장",
  topic: "주제",
  shared: "공용",
  unknown: "기록 없음",
};

/** 표마다 열 너비를 같게 둔다(위아래 표의 숫자 열이 나란히 보이게). */
function UsageTable({
  title,
  head,
  testId,
  children,
}: {
  title?: string;
  head: string;
  testId: string;
  children: ReactNode;
}) {
  return (
    <table className="mt-2 w-full table-fixed text-sm" data-testid={testId}>
      {title && (
        <caption className="text-left text-sm font-medium">{title}</caption>
      )}
      <thead className="text-left text-xs text-muted-foreground">
        <tr className="border-b">
          <th className="py-1.5 font-normal">{head}</th>
          <th className="w-16 py-1.5 text-right font-normal">호출</th>
          <th className="w-20 py-1.5 text-right font-normal">비용</th>
          <th className="w-12 py-1.5 text-right font-normal">비율</th>
        </tr>
      </thead>
      <tbody>{children}</tbody>
    </table>
  );
}

function UsageLine({
  calls,
  costUsd,
  share,
  children,
}: {
  calls: number;
  costUsd: number;
  share: number;
  children: ReactNode;
}) {
  return (
    <tr className="border-b">
      <td className="py-1.5 break-words">{children}</td>
      <td className="py-1.5 text-right tabular-nums">
        {calls.toLocaleString("ko-KR")}회
      </td>
      <td className="py-1.5 text-right tabular-nums">${costUsd.toFixed(4)}</td>
      <td className="py-1.5 text-right tabular-nums">{formatShare(share)}</td>
    </tr>
  );
}

/** 0.5% 미만은 '<1%'로 */
function formatShare(share: number): string {
  if (share <= 0) return "0%";
  const percent = share * 100;
  return percent < 0.5 ? "<1%" : `${Math.round(percent)}%`;
}
