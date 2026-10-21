import { cn } from "cn";
import type { BudgetLevel, UsageRow } from "@/server/budget/report";

const PROVIDER_LABEL = { openrouter: "OpenRouter", typesafe: "TypeSafe(Jev)" };

const BAR_CLASS: Record<BudgetLevel, string> = {
  ok: "bg-primary",
  warn: "bg-amber-500",
  over: "bg-destructive",
};

/** 이번 달 AI 사용량: 제공자·용도별 호출 수와 비용, 한도 대비 막대(2026-10-21 결정) */
export function UsageView({
  report,
}: {
  report: {
    rows: UsageRow[];
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
        <table className="w-full text-sm">
          <thead className="text-left text-xs text-muted-foreground">
            <tr className="border-b">
              <th className="py-1.5 font-normal">용도</th>
              <th className="py-1.5 text-right font-normal">호출</th>
              <th className="py-1.5 text-right font-normal">비용</th>
            </tr>
          </thead>
          <tbody>
            {report.rows.map((row) => (
              <tr key={`${row.provider}:${row.purpose}`} className="border-b">
                <td className="py-1.5">
                  {row.label}
                  <span className="text-xs text-muted-foreground">
                    {" "}
                    · {PROVIDER_LABEL[row.provider]}
                  </span>
                </td>
                <td className="py-1.5 text-right tabular-nums">
                  {row.calls.toLocaleString("ko-KR")}회
                </td>
                <td className="py-1.5 text-right tabular-nums">
                  ${row.costUsd.toFixed(4)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
