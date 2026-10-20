import { expect, test } from "@playwright/test";
import { asClient, login, searchStock, waitForListings } from "./helpers";

// 가짜 외부 서비스(공식 목록·OpenRouter)가 필요한 테스트라 로컬에서만 돈다.
// Docker에서는 실제 키로 따로 확인한다(유료 호출이라 허락 범위 안에서만).
test.skip(
  Boolean(process.env.E2E_BASE_URL),
  "가짜 외부 서비스가 필요한 테스트라 로컬 E2E에서만 실행한다",
);

// 데스크톱과 모바일이 동시에 돌므로 서로 다른 종목을 쓴다.
const CASES = {
  desktop: {
    query: "삼전",
    name: "삼성전자",
    code: "005930",
    exchange: "KOSPI",
  },
  mobile: { query: "NVDA", name: "엔비디아", code: "NVDA", exchange: "NASDAQ" },
} as const;

test.describe("종목 등록·삭제 (SPEC FR-2, D-11, D-12)", () => {
  test.beforeEach(async () => {
    await waitForListings();
  });

  test("입력으로 후보를 찾아 등록하고, 확인창을 거쳐 삭제한다", async ({
    page,
  }, testInfo) => {
    const target = CASES[testInfo.project.name as keyof typeof CASES];
    await asClient(page, testInfo, "stocks");
    await login(page, "/stocks");
    await expect(
      page.getByRole("heading", { name: "종목", exact: true }),
    ).toBeVisible();

    await searchStock(page, target.query);
    const candidate = page
      .getByTestId("candidate")
      .filter({ hasText: target.name });
    await expect(candidate).toContainText(target.code);
    await expect(candidate).toContainText(target.exchange);
    await page.screenshot({
      path: testInfo.outputPath("stocks-candidate.png"),
      fullPage: true,
    });

    await candidate
      .getByRole("button", { name: `${target.name} 등록` })
      .click();
    await expect(candidate).toContainText("등록함");
    const item = page.getByTestId("stock").filter({ hasText: target.name });
    await expect(item).toBeVisible();
    await page.screenshot({
      path: testInfo.outputPath("stocks-registered.png"),
      fullPage: true,
    });

    // 같은 종목을 다시 찾으면 이미 등록했다고 보여 준다.
    await searchStock(page, target.query);
    await expect(
      page.getByTestId("candidate").filter({ hasText: target.name }),
    ).toContainText("이미 등록됨");

    await item.getByRole("button", { name: `${target.name} 삭제` }).click();
    const dialog = page.getByRole("alertdialog");
    await expect(dialog).toContainText(`${target.name} 종목을 삭제할까요?`);
    await page.screenshot({
      path: testInfo.outputPath("stocks-delete-confirm.png"),
      fullPage: true,
    });
    await dialog.getByRole("button", { name: "삭제" }).click();
    await expect(
      page.getByTestId("stock").filter({ hasText: target.name }),
    ).toHaveCount(0);
  });

  test("모르는 입력이면 찾지 못했다고 알려 준다", async ({
    page,
  }, testInfo) => {
    await asClient(page, testInfo, "stocks-unknown");
    await login(page, "/stocks");
    await searchStock(page, "없는종목이름");
    await expect(page.getByText("맞는 종목을 찾지 못했습니다.")).toBeVisible();
  });
});
