import { expect, test } from "@playwright/test";
import { asClient, login } from "./helpers";

test("상태 화면(/status): web이 DB(읽기 전용)와 worker 내부 API에 닿는다", async ({
  page,
}, testInfo) => {
  await asClient(page, testInfo, "home");
  await login(page, "/status");

  await expect(
    page.getByRole("heading", { name: "시스템 상태" }),
  ).toBeVisible();
  // 스트리밍된 결과는 숨겨진 채 DOM에 먼저 들어오므로 "보이는지"까지 기다린다.
  const dbStatus = page.getByTestId("db-status");
  const workerStatus = page.getByTestId("worker-status");
  await expect(dbStatus).toBeVisible();
  await expect(workerStatus).toBeVisible();
  await expect(page.getByText("확인 중")).toBeHidden();
  await expect(dbStatus).toContainText("정상");
  await expect(dbStatus).toContainText("journal_mode wal");
  await expect(workerStatus).toContainText("정상");
  // 공개 URL로 노출되므로 검색엔진 수집을 막는다.
  await expect(page.locator('meta[name="robots"]')).toHaveAttribute(
    "content",
    "noindex, nofollow",
  );

  await page.screenshot({
    path: testInfo.outputPath("home.png"),
    fullPage: true,
  });
});
