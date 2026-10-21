import Link from "next/link";
import { requireSession } from "@/server/auth/guard";
import { budgetLevel } from "@/server/budget/report";
import { monthSpendUsd } from "@/server/budget/usage";
import { getWebDb } from "@/server/db/web";
import { readSetting } from "@/server/settings";

/** 월 예산 80% 넘음(노랑)·한도 도달(빨강)을 모든 화면 위에 알린다(D-07). */
export async function BudgetBanner() {
  await requireSession();
  const db = getWebDb();
  const spent = monthSpendUsd(db);
  const limit = readSetting(db, "budget.monthlyUsd");
  const level = budgetLevel(spent, limit);
  if (level === "ok") return null;
  const over = level === "over";
  return (
    <div
      role={over ? "alert" : "status"}
      data-testid="budget-banner"
      className={
        over
          ? "border-b border-destructive/30 bg-destructive/10 text-destructive"
          : "border-b border-amber-300 bg-amber-50 text-amber-900 dark:border-amber-800 dark:bg-amber-950/40 dark:text-amber-200"
      }
    >
      <p className="mx-auto w-full max-w-xl px-4 py-2 text-sm">
        {over
          ? `이번 달 AI 예산($${limit})을 다 써서 기사 평가·키워드 만들기·종목 찾기를 멈췄습니다. 수집은 계속합니다. `
          : `이번 달 AI 예산의 ${Math.floor((spent / limit) * 100)}%를 썼습니다($${spent.toFixed(2)} / $${limit}). `}
        <Link
          href="/settings"
          className="font-medium underline underline-offset-2"
        >
          {over ? "설정에서 한도 올리기" : "설정 보기"}
        </Link>
      </p>
    </div>
  );
}
