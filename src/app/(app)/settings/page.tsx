import { Suspense } from "react";
import { requireSession } from "@/server/auth/guard";
import { monthUsageReport } from "@/server/budget/report";
import { getWebDb } from "@/server/db/web";
import { stocksWithOtherDepth } from "@/server/keywords/read";
import { readSetting } from "@/server/settings";
import { readSettingsForm, settingsFormText } from "@/server/settings-form";
import { SETTINGS_FIELDS, type SettingsValues } from "./fields";
import { RebuildKeywordsButton } from "./rebuild-keywords-button";
import { SettingsForm } from "./settings-form";
import { UsageView } from "./usage-view";

// 설정 화면(SPEC FR-8, 8단계). 값은 worker가 저장하고 web은 읽기만 한다(D-36).
export default function SettingsPage() {
  return (
    <main className="mx-auto w-full max-w-xl px-4 py-8 sm:py-12">
      <h1 className="text-xl font-semibold">설정</h1>
      <p className="mt-1 text-sm text-muted-foreground">
        수집·키워드·평가·비용 설정과 이번 달 AI 사용량입니다.
      </p>
      <Suspense
        fallback={
          <p className="mt-6 text-sm text-muted-foreground">불러오는 중…</p>
        }
      >
        <Settings />
      </Suspense>
    </main>
  );
}

async function Settings() {
  // 데이터에 닿기 전에 로그인을 한 번 더 확인한다(요청 시점 실행).
  await requireSession();
  const db = getWebDb();
  const form = readSettingsForm(db);
  const text = settingsFormText(form);
  const initial = Object.fromEntries(
    SETTINGS_FIELDS.map((field) => [field, text[field]]),
  ) as SettingsValues;
  const rebuild = stocksWithOtherDepth(db).length;

  return (
    <>
      <div className="mt-6">
        <SettingsForm initial={initial} />
      </div>

      <section className="mt-10 space-y-2" data-testid="keyword-rebuild">
        <h2 className="text-base font-semibold">키워드 다시 만들기</h2>
        <p className="text-sm text-muted-foreground">
          {rebuild > 0
            ? `지금 깊이(${form.keywordsDepth}단계)와 다르게 만든 종목·시장·주제가 ${rebuild}개 있습니다. 누르면 바로 다시 만듭니다(후보 트리와 Jev 평가, 하나에 약 $0.003).`
            : `모든 종목·시장·주제의 키워드가 지금 깊이(${form.keywordsDepth}단계)로 만들어져 있습니다.`}
        </p>
        <RebuildKeywordsButton count={rebuild} />
      </section>

      <section className="mt-10 space-y-2">
        <h2 className="text-base font-semibold">기사 평가 모델(Jev)</h2>
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
          <dt className="text-muted-foreground">모델</dt>
          <dd className="font-mono">{readSetting(db, "jev.model")}</dd>
          <dt className="text-muted-foreground">단가</dt>
          <dd>
            입력 100만 토큰당 ${readSetting(db, "jev.usdPerMillionInputTokens")}
            (출력 무료)
          </dd>
        </dl>
        <p className="text-xs text-muted-foreground">
          버전을 고정해 씁니다(D-23). 바꿀 때는 품질을 다시 비교하므로
          화면에서는 바꾸지 않습니다.
        </p>
      </section>

      <UsageView report={monthUsageReport(db)} />
    </>
  );
}
