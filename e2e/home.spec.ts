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
  // 외부 접속 임시 주소와 마지막 백업(9단계). 로컬 E2E에는 터널·백업 폴더가 없다.
  const tunnel = page.getByTestId("tunnel-status");
  const backup = page.getByTestId("backup-status");
  if (process.env.E2E_BASE_URL) {
    await expect(tunnel).toBeVisible();
    await expect(backup).toBeVisible();
  } else {
    await expect(tunnel).toHaveText(
      "터널을 쓰지 않습니다(이 컴퓨터에서만 접속).",
    );
    await expect(backup).toContainText("아직 없습니다");
  }
  // 시장 토픽 갱신 상태와 "지금 갱신"(11단계)
  const topics = page.getByTestId("topic-refresh");
  await expect(topics).toContainText("토픽 갱신");
  await expect(
    topics.getByRole("button", { name: /지금 갱신|갱신 중…/ }),
  ).toBeVisible();
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
