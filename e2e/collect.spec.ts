import { expect, test } from "@playwright/test";
import { asClient, login, registerStock, waitForListings } from "./helpers";

// 가짜 언론사 RSS·Finnhub가 필요한 테스트라 로컬에서만 돈다.
test.skip(
  Boolean(process.env.E2E_BASE_URL),
  "가짜 외부 서비스가 필요한 테스트라 로컬 E2E에서만 실행한다",
);

test.describe("기사 수집 (SPEC FR-4)", () => {
  test("상태 화면에서 '지금 수집'을 누르면 종목에 걸린 기사를 모으고 출처별 결과를 보여 준다", async ({
    page,
  }, testInfo) => {
    test.skip(
      testInfo.project.name !== "desktop",
      "수집은 앱 전체에 하나라서 한 번만 돈다",
    );
    await waitForListings();
    await asClient(page, testInfo, "collect");
    await login(page, "/stocks");
    await registerStock(page, "하닉", "SK하이닉스");

    await page.goto("/");
    const panel = page.getByTestId("collection");
    await expect(panel).toBeVisible();
    await panel.getByRole("button", { name: "지금 수집" }).click();
    // 같은 사이트에는 요청 간격(1.1초)을 두므로 바로 끝나지 않는다.
    await expect(panel.getByRole("button", { name: "수집 중…" })).toBeVisible();
    await page.screenshot({
      path: testInfo.outputPath("collect-running.png"),
      fullPage: true,
      animations: "disabled",
    });

    const run = page.getByTestId("collection-run");
    await expect(run).toContainText("완료", { timeout: 30_000 });
    await expect(
      panel.getByRole("button", { name: "지금 수집" }),
    ).toBeEnabled();
    // 이름으로 걸린 SK하이닉스 기사 2건, 키워드로 걸린 기사, Finnhub 기사가 저장된다.
    await expect(run).toContainText(/새 기사 [1-9]\d*건/);
    await expect(panel).not.toContainText("저장된 기사 0건");

    await run.getByText(/출처별 결과 \d+곳/).click();
    const sources = run.getByTestId("collection-sources");
    await expect(sources).toContainText("가짜경제");
    await expect(sources).toContainText("가짜뉴스");
    await expect(sources).toContainText("Finnhub");
    await page.screenshot({
      path: testInfo.outputPath("collect-done.png"),
      fullPage: true,
      animations: "disabled",
    });
  });
});
