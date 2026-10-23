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

// 데스크톱과 모바일이 동시에 돌므로 서로 다른 주제를 쓴다.
const TOPICS = { desktop: "부동산 시장", mobile: "중동 정세" } as const;

test.describe("시장·주제 (11단계)", () => {
  test.beforeEach(async () => {
    await waitForListings();
  });

  test("한국 증시는 켜진 채로 트리를 만들고, 주제를 넣으면 트리를 만들며, 확인창을 거쳐 지운다", async ({
    page,
  }, testInfo) => {
    const topic = TOPICS[testInfo.project.name as keyof typeof TOPICS];
    await asClient(page, testInfo, "market-topics");
    await login(page, "/stocks");
    const section = page.getByTestId("market-topics");
    await expect(
      section.getByRole("heading", { name: "시장·주제" }),
    ).toBeVisible();

    const kr = section.getByTestId("market").filter({ hasText: "한국 증시" });
    await expect(kr).toContainText("KOSPI·KOSDAQ");
    await expect(
      kr.getByRole("button", { name: "한국 증시 끄기" }),
    ).toBeVisible();
    // 지수 이름이 아니라 시장을 움직이는 요인으로 트리를 만든다(가짜 AI는 종목과 같은 트리를 준다).
    await expect(kr.getByText(/키워드 \d+개/).first()).toBeVisible({
      timeout: 20_000,
    });
    await expect(
      section.getByTestId("market").filter({ hasText: "미국 증시" }),
    ).toBeVisible();

    await section.getByLabel("주제", { exact: true }).fill(topic);
    await section.getByRole("button", { name: "주제 추가" }).click();
    const item = section.getByTestId("user-topic").filter({ hasText: topic });
    await expect(item).toBeVisible();
    // 넣고 나면 입력칸을 비운다.
    await expect(section.getByLabel("주제", { exact: true })).toHaveValue("");
    await expect(item.getByText(/키워드 \d+개/).first()).toBeVisible({
      timeout: 20_000,
    });
    await item
      .getByText(/키워드 \d+개/)
      .first()
      .click();
    await expect(
      item.getByTitle(`${topic} → AI 반도체`, { exact: true }).first(),
    ).toBeVisible();

    // 같은 주제는 두 번 넣지 않는다.
    await section
      .getByLabel("주제", { exact: true })
      .fill(topic.replace(" ", ""));
    await section.getByRole("button", { name: "주제 추가" }).click();
    await expect(section.getByRole("alert")).toHaveText(
      "이미 넣은 주제입니다.",
    );
    await page.screenshot({
      path: testInfo.outputPath("market-topics.png"),
      fullPage: true,
      animations: "disabled",
    });

    await item.getByRole("button", { name: `${topic} 삭제` }).click();
    const dialog = page.getByRole("alertdialog");
    await expect(dialog).toContainText(`${topic} 주제를 삭제할까요?`);
    await dialog.getByRole("button", { name: "삭제" }).click();
    await expect(
      section.getByTestId("user-topic").filter({ hasText: topic }),
    ).toHaveCount(0);
  });

  test("미국 증시를 켜면 미국 기준 트리를 만들고, 끄면 숨긴다", async ({
    page,
  }, testInfo) => {
    test.skip(
      testInfo.project.name !== "desktop",
      "앱 전체 시장 설정을 바꾸므로 한 번만 돈다",
    );
    await asClient(page, testInfo, "market-us");
    await login(page, "/stocks");
    const us = page
      .getByTestId("market-topics")
      .getByTestId("market")
      .filter({ hasText: "미국 증시" });
    await expect(us).toContainText("꺼짐");
    await us.getByRole("button", { name: "미국 증시 켜기" }).click();
    await expect(us).toContainText("S&P 500·NASDAQ");
    await expect(us.getByText(/키워드 \d+개/)).toBeVisible({ timeout: 20_000 });
    // 미국 기준 트리는 영어 표기도 만든다.
    await us.getByText(/키워드 \d+개/).click();
    await expect(us.getByText("AI chips")).toBeVisible();
    await page.screenshot({
      path: testInfo.outputPath("market-us.png"),
      fullPage: true,
      animations: "disabled",
    });
    await us.getByRole("button", { name: "미국 증시 끄기" }).click();
    await expect(us).toContainText("꺼짐");
  });
});
