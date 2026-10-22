import { expect, test } from "@playwright/test";
import { asClient, login, registerStock, waitForListings } from "./helpers";

// 가짜 OpenRouter가 필요한 테스트라 로컬에서만 돈다.
test.skip(
  Boolean(process.env.E2E_BASE_URL),
  "가짜 외부 서비스가 필요한 테스트라 로컬 E2E에서만 실행한다",
);

// 다른 테스트와 같은 worker 대기열을 쓰므로 넉넉히 기다린다.
const KEYWORDS_TIMEOUT = 20_000;

// 데스크톱과 모바일이 동시에 돌므로 서로 다른 종목을 쓴다.
const CASES = {
  desktop: { query: "카카오", name: "카카오", english: false },
  mobile: { query: "TSLA", name: "테슬라", english: true },
} as const;

test.describe("키워드 생성·트리 보기 (SPEC FR-3, D-13~D-16, D-37)", () => {
  test.beforeEach(async () => {
    await waitForListings();
  });

  test("등록하면 키워드를 만들고, 펼쳐서 경로가 있는 트리를 본다", async ({
    page,
  }, testInfo) => {
    const target = CASES[testInfo.project.name as keyof typeof CASES];
    await asClient(page, testInfo, "keywords");
    await login(page, "/stocks");

    const item = await registerStock(page, target.query, target.name);
    // 등록은 바로 끝나고, 키워드는 worker가 따로 만든다.
    await expect(item).toContainText("키워드를 만드는 중…");
    await page.screenshot({
      path: testInfo.outputPath("keywords-making.png"),
      fullPage: true,
      animations: "disabled",
    });

    // 화면이 스스로 다시 읽어 완료를 보여 준다.
    const summary = item.getByText("키워드 10개");
    await expect(summary).toBeVisible({ timeout: KEYWORDS_TIMEOUT });
    await expect(item).toContainText("깊이 2");
    await expect(item).not.toContainText("키워드를 만드는 중…");

    await summary.click();
    const branch = item.getByTestId("keyword").filter({
      has: page.getByTitle(`${target.name} → AI 반도체`, { exact: true }),
    });
    await expect(branch).toContainText("AI 서버 수요와 직결");
    // 2단계 키워드는 부모 아래에 들어가고, 경로는 종목 이름부터 이어진다.
    await expect(
      branch.getByTitle(`${target.name} → AI 반도체 → HBM`, { exact: true }),
    ).toBeVisible();
    await expect(item.getByTestId("keyword")).toHaveCount(10);
    // 시사 가지는 맨 위 키워드에만 '시사'를 붙이고, 아래 키워드는 그 가지 안에 든다(10단계).
    const current = item.getByTestId("keyword").filter({
      has: page.getByTitle(`${target.name} → 금리 인하 기대`, { exact: true }),
    });
    await expect(item.getByTestId("current-badge")).toHaveCount(1);
    await expect(current.getByTestId("current-badge").first()).toHaveText(
      "시사",
    );
    await expect(
      current.getByTitle(`${target.name} → 금리 인하 기대 → 연준 금리`, {
        exact: true,
      }),
    ).toBeVisible();
    // 영어 표기는 미국 종목에만 있다(D-14).
    await expect(item.getByText("AI chips")).toHaveCount(
      target.english ? 1 : 0,
    );
    await page.screenshot({
      path: testInfo.outputPath("keywords-tree.png"),
      fullPage: true,
      animations: "disabled",
    });

    await item
      .getByRole("button", { name: `${target.name} 키워드 다시 만들기` })
      .click();
    await expect(item).toContainText("키워드를 만드는 중…");
    // 다시 만드는 동안에도 이전 키워드는 보인다.
    await expect(item.getByTestId("keyword")).toHaveCount(10);
    await expect(item).not.toContainText("키워드를 만드는 중…", {
      timeout: KEYWORDS_TIMEOUT,
    });
    await expect(
      item.getByRole("button", { name: `${target.name} 키워드 다시 만들기` }),
    ).toBeVisible();
  });

  test("만들지 못하면 이유를 보여 주고, 다시 만들 수 있다", async ({
    page,
  }, testInfo) => {
    test.skip(
      testInfo.project.name !== "desktop",
      "가짜 OpenRouter가 첫 요청만 실패시키므로 한 번만 돈다",
    );
    await asClient(page, testInfo, "keywords-failed");
    await login(page, "/stocks");

    const item = await registerStock(page, "현대건설", "현대건설");
    await expect(item).toContainText(
      "키워드를 만들지 못했습니다. AI 서비스에 연결하지 못했거나 오류가 났습니다.",
      { timeout: KEYWORDS_TIMEOUT },
    );
    await page.screenshot({
      path: testInfo.outputPath("keywords-failed.png"),
      fullPage: true,
      animations: "disabled",
    });

    await item
      .getByRole("button", { name: "현대건설 키워드 다시 만들기" })
      .click();
    await expect(item.getByText("키워드 10개")).toBeVisible({
      timeout: KEYWORDS_TIMEOUT,
    });
    await expect(item).not.toContainText("만들지 못했습니다");
  });
});
