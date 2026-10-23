import Database from "better-sqlite3";
import { expect, test, type Page } from "@playwright/test";
import { asClient, login } from "./helpers";
import { E2E_DB_PATH } from "./test-env";

// 설정을 바꾸거나 테스트 DB에 사용량을 직접 넣으므로 로컬 E2E에서만 돈다.
test.skip(
  Boolean(process.env.E2E_BASE_URL),
  "설정을 바꾸고 테스트 DB를 고치는 테스트라 로컬 E2E에서만 실행한다",
);

async function openSettings(page: Page) {
  await page
    .getByRole("navigation", { name: "메뉴" })
    .getByRole("link", { name: "설정" })
    .click();
  await expect(
    page.getByRole("heading", { name: "설정", level: 1 }),
  ).toBeVisible();
  return page.getByTestId("settings-form");
}

test.describe("설정 화면 (SPEC FR-8)", () => {
  test("항목과 이번 달 사용량, Jev 모델을 보여 준다", async ({
    page,
  }, testInfo) => {
    await asClient(page, testInfo, "settings-view");
    await login(page, "/");
    const form = await openSettings(page);
    await expect(form.getByLabel("수집 간격")).toBeVisible();
    await expect(form.getByLabel("월 예산 한도(달러)")).toBeVisible();
    // 11단계: 표시 기준(관련도·영향도), 키워드를 남길 기준, 토픽 갱신 간격·시각
    await expect(form.getByLabel("관련도 표시 기준(0~10)")).toHaveValue("6");
    await expect(form.getByLabel("영향도 표시 기준(0~10)")).toHaveValue("6");
    await expect(form.getByLabel("키워드 남길 관련도(0~10)")).toHaveValue("6");
    await expect(form.getByLabel("키워드 남길 영향도(0~10)")).toHaveValue("5");
    await expect(form.getByLabel("토픽 갱신 간격(시간, 0~24)")).toHaveValue(
      "3",
    );
    await expect(
      form.getByLabel("토픽 갱신 특정 시각(한국 시간)"),
    ).toBeVisible();
    await expect(page.getByTestId("usage")).toContainText("이번 달 AI 사용량");
    // 기능별·항목별 비용과 비율(11단계). worker가 시작하며 한국 증시 트리를 만들므로 시장 비용이 생긴다.
    await expect(async () => {
      await page.reload();
      await expect(page.getByTestId("usage-groups")).toContainText("시장", {
        timeout: 1000,
      });
    }).toPass({ timeout: 20_000 });
    await expect(page.getByTestId("usage-features")).toContainText("%");
    await expect(page.getByTestId("usage-groups")).toContainText("%");
    await expect(page.getByText("jev-1.13.0")).toBeVisible();
    await expect(page.getByTestId("keyword-rebuild")).toBeVisible();
    await page.screenshot({
      path: testInfo.outputPath("settings.png"),
      fullPage: true,
      animations: "disabled",
    });
  });
});

// 설정 값과 사용량은 앱 전체에 하나라서, 바꾸는 테스트는 데스크톱에서만 차례로 돈다.
test.describe("설정 바꾸기와 예산 알림 (FR-8, D-07)", () => {
  test.describe.configure({ mode: "serial" });

  test("저장한 값은 다시 열어도 남고, OpenRouter에 없는 모델 ID는 저장하지 않는다", async ({
    page,
  }, testInfo) => {
    test.skip(
      testInfo.project.name !== "desktop",
      "앱 전체 설정을 바꾸므로 한 번만 돈다",
    );
    await asClient(page, testInfo, "settings-save");
    await login(page, "/");
    let form = await openSettings(page);
    await expect(form.getByLabel("수집 간격")).toHaveValue("30");
    await expect(
      form.getByLabel("키워드·종목 찾기 모델(OpenRouter)"),
    ).toHaveValue("anthropic/claude-haiku-5.5");

    await form.getByLabel("수집 간격").selectOption("60");
    await form.getByLabel("자동 재생성 주기").selectOption("weekly");
    await form.getByLabel("월 예산 한도(달러)").fill("7.5");
    await form.getByRole("button", { name: "저장" }).click();
    await expect(form.getByRole("status")).toHaveText("저장했습니다.");

    await page.reload();
    form = page.getByTestId("settings-form");
    await expect(form.getByLabel("수집 간격")).toHaveValue("60");
    await expect(form.getByLabel("자동 재생성 주기")).toHaveValue("weekly");
    await expect(form.getByLabel("월 예산 한도(달러)")).toHaveValue("7.5");
    await expect(page.getByTestId("usage")).toContainText("한도 $7.5");

    // 없는 모델: 저장하지 않고 이유를 보여 준다(넣은 값은 그대로).
    const model = form.getByLabel("키워드·종목 찾기 모델(OpenRouter)");
    await model.fill("nobody/none");
    await form.getByRole("button", { name: "저장" }).click();
    await expect(page.getByTestId("llmModel-error")).toHaveText(
      "OpenRouter에 없는 모델 ID입니다.",
    );
    await expect(model).toHaveValue("nobody/none");
    await page.screenshot({
      path: testInfo.outputPath("settings-error.png"),
      fullPage: true,
      animations: "disabled",
    });

    // 토픽 갱신 특정 시각(11단계): 틀린 시각은 이유를 보이고, 한 자리 시는 두 자리로 맞춰 시각 순으로 남긴다.
    await model.fill("anthropic/claude-haiku-5.5");
    const times = form.getByLabel("토픽 갱신 특정 시각(한국 시간)");
    await times.fill("25:00");
    await form.getByRole("button", { name: "저장" }).click();
    await expect(page.getByTestId("topicTimes-error")).toHaveText(
      "09:00, 15:30처럼 24시간제 HH:MM으로 넣으세요.",
    );
    await times.fill("15:40, 8:30");
    await form.getByRole("button", { name: "저장" }).click();
    await expect(form.getByRole("status")).toHaveText("저장했습니다.");
    await page.reload();
    form = page.getByTestId("settings-form");
    await expect(form.getByLabel("토픽 갱신 특정 시각(한국 시간)")).toHaveValue(
      "08:30, 15:40",
    );
    await form.getByLabel("토픽 갱신 특정 시각(한국 시간)").fill("");

    // 원래 값으로 되돌린다(다른 테스트가 기본값을 쓴다).
    await form
      .getByLabel("키워드·종목 찾기 모델(OpenRouter)")
      .fill("anthropic/claude-haiku-5.5");
    await form.getByLabel("수집 간격").selectOption("30");
    await form.getByLabel("자동 재생성 주기").selectOption("daily");
    await form.getByLabel("월 예산 한도(달러)").fill("5");
    await form.getByRole("button", { name: "저장" }).click();
    await expect(form.getByRole("status")).toHaveText("저장했습니다.");
  });

  test("이번 달 사용이 한도의 80%를 넘으면 모든 화면 위에 알린다", async ({
    page,
  }, testInfo) => {
    test.skip(
      testInfo.project.name !== "desktop",
      "앱 전체 사용량을 바꾸므로 한 번만 돈다",
    );
    // 다른 테스트의 AI 호출을 막지 않도록 한도(5달러) 아래인 4.2달러만 넣고 바로 지운다.
    const db = new Database(E2E_DB_PATH, { timeout: 10_000 });
    const row = db
      .prepare(
        `insert into usage_logs (at, provider, model, purpose, input_tokens, output_tokens, cost_usd)
         values (?, 'openrouter', 'e2e', 'e2e_budget_banner', 0, 0, 4.2)`,
      )
      .run(Date.now()).lastInsertRowid;
    try {
      await asClient(page, testInfo, "budget-banner");
      await login(page, "/");
      const banner = page.getByTestId("budget-banner");
      await expect(banner).toContainText(/이번 달 AI 예산의 8\d%를 썼습니다/);
      await expect(
        banner.getByRole("link", { name: "설정 보기" }),
      ).toHaveAttribute("href", "/settings");
      await page.screenshot({
        path: testInfo.outputPath("budget-banner.png"),
        fullPage: false,
        animations: "disabled",
      });
    } finally {
      db.prepare("delete from usage_logs where id = ?").run(row);
      db.close();
    }
    await page.reload();
    await expect(page.getByTestId("budget-banner")).toHaveCount(0);
  });
});
