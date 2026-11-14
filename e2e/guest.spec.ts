import Database from "better-sqlite3";
import { expect, test } from "@playwright/test";
import {
  asClient,
  login,
  searchStock,
  seedNatureFixtures,
  waitForListings,
} from "./helpers";
import { E2E_APP_PASSWORD, E2E_DB_PATH } from "./test-env";

// 손님 보기·비밀번호 창·손님 읽음(16단계). 손님 GET은 IP별 1분 60번이라 시험마다 접속자를 따로 쓰고 많이 열지 않는다.
const QUERY = { desktop: "삼전", mobile: "NVDA" } as const;
const MIN = 60 * 1000;

const cards = (page: import("@playwright/test").Page) =>
  page.getByTestId("timeline-card");

/** 손님 읽음 시험용으로 프로젝트마다 종목 하나와 기사 하나를 넣는다(이미 있으면 그대로 둔다). */
function seedGuestFixtures(project: string): {
  stockName: string;
  title: string;
  articleId: number;
} {
  const db = new Database(E2E_DB_PATH, { timeout: 10_000 });
  const stockName = `손님시험${project}`;
  const title = `${project} 손님이 읽는 소식 한 건`;
  try {
    return db
      .transaction(() => {
        const code = `GUEST-${project}`;
        const found = db
          .prepare<[string], { id: number }>(
            "select id from stocks where market = 'KR' and code = ?",
          )
          .get(code);
        if (found) {
          const article = db
            .prepare<[string], { id: number }>(
              "select id from articles where title = ?",
            )
            .get(title)!;
          return { stockName, title, articleId: article.id };
        }
        const now = Date.now();
        const stock = Number(
          db
            .prepare(
              `insert into stocks (market, code, name, name_ko, exchange, user_input, created_at, keywords_status, keywords_generated_at)
               values ('KR', ?, ?, ?, 'KOSDAQ', ?, ?, 'ready', ?)`,
            )
            .run(code, stockName, stockName, stockName, now, now)
            .lastInsertRowid,
        );
        const key = `guest.example/${project}`;
        const articleId = Number(
          db
            .prepare(
              `insert into articles (url_key, url, title, publisher, feed, published_at, collected_at)
               values (?, ?, ?, '가짜경제', 'rss:mock:economy', ?, ?)`,
            )
            .run(key, `https://${key}`, title, now - 5 * MIN, now)
            .lastInsertRowid,
        );
        const matchId = Number(
          db
            .prepare(
              `insert into article_matches (article_id, stock_id, keyword_path, match_kind, level, created_at)
               values (?, ?, ?, 'name', 0, ?)`,
            )
            .run(articleId, stock, stockName, now).lastInsertRowid,
        );
        db.prepare(
          `insert into evaluations (match_id, article_id, stock_id, relevance, relevance_confidence, magnitude, impact, impact_confidence, impact_probabilities, model, evaluated_at)
           values (?, ?, ?, 8, 0.7, 7, 'positive', 0.8, '{}', 'jev-1.13.0', ?)`,
        ).run(matchId, articleId, stock, now);
        return { stockName, title, articleId };
      })
      .immediate();
  } finally {
    db.close();
  }
}

test.describe("손님 보기와 비밀번호 창 (16단계)", () => {
  test("손님은 타임라인·종목을 비밀번호 없이 보고, 설정은 로그인 화면으로 간다", async ({
    page,
  }, testInfo) => {
    await asClient(page, testInfo, "guest-view");
    await page.goto("/");
    await expect(page.getByRole("heading", { name: "타임라인" })).toBeVisible();
    await expect(page.locator("body")).toHaveAttribute("data-access", "guest");
    await expect(page.getByRole("button", { name: "로그아웃" })).toHaveCount(0);
    await page.screenshot({
      path: testInfo.outputPath("guest-timeline.png"),
      fullPage: true,
    });

    await page.goto("/stocks");
    await expect(
      page.getByRole("heading", { name: "종목", exact: true }),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "로그아웃" })).toHaveCount(0);

    await page.getByRole("link", { name: "설정" }).click();
    await expect(page).toHaveURL(/\/login\?next=%2Fsettings$/);
  });

  test("수정하면 비밀번호 창이 뜨고, 맞으면 같은 요청이 다시 실행된다", async ({
    page,
  }, testInfo) => {
    await waitForListings();
    const query = QUERY[testInfo.project.name as keyof typeof QUERY];
    await asClient(page, testInfo, "guest-unlock");
    await page.goto("/stocks");
    const dialog = page.getByRole("alertdialog", { name: "비밀번호 확인" });
    await expect(dialog).toBeHidden();

    await searchStock(page, query);
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText("수정하려면 비밀번호를 입력하세요.");
    await expect(page.getByTestId("candidate")).toHaveCount(0);
    // 제출 단추는 풀려 있다(응답이 바꿔 끼워지지 않아도 "찾는 중…"으로 남지 않는다).
    await expect(page.getByRole("button", { name: "찾는 중…" })).toHaveCount(0);

    // 틀린 비밀번호는 창 안에 오류를 보이고 창은 열려 있다.
    await dialog.getByLabel("비밀번호").fill("wrong-password-123");
    await dialog.getByRole("button", { name: "확인" }).click();
    await expect(dialog.getByRole("alert")).toHaveText(
      "비밀번호가 맞지 않습니다.",
    );
    await expect(dialog).toBeVisible();
    await page.screenshot({
      path: testInfo.outputPath("unlock-dialog-error.png"),
      fullPage: true,
    });

    // 맞는 비밀번호: 창이 닫히고 같은 검색이 실행된다(입력 값 유지).
    await dialog.getByLabel("비밀번호").fill(E2E_APP_PASSWORD);
    await dialog.getByRole("button", { name: "확인" }).click();
    await expect(dialog).toBeHidden();
    await expect(page.getByTestId("candidate").first()).toBeVisible();
    await expect(page.getByLabel("종목 이름 또는 티커")).toHaveValue(query);

    // 주인 로그인이 생겨 상태·설정 화면이 열린다.
    await page.goto("/status");
    await expect(page).toHaveURL(/\/status$/);
    await page.goto("/settings");
    await expect(page).toHaveURL(/\/settings$/);
  });

  test("취소하면 창이 닫히고 아무것도 실행되지 않는다", async ({
    page,
  }, testInfo) => {
    await asClient(page, testInfo, "guest-cancel");
    await page.goto("/stocks");
    const dialog = page.getByRole("alertdialog", { name: "비밀번호 확인" });
    await searchStock(page, "삼전");
    await expect(dialog).toBeVisible();
    await page.screenshot({
      path: testInfo.outputPath("unlock-dialog.png"),
      fullPage: true,
    });
    await dialog.getByRole("button", { name: "취소" }).click();
    await expect(dialog).toBeHidden();
    await expect(page.getByTestId("candidate")).toHaveCount(0);
  });

  test("삭제 확인창 위에서도 비밀번호 창이 뜬다", async ({
    page,
  }, testInfo) => {
    await waitForListings();
    const { stockName } = seedGuestFixtures(testInfo.project.name);
    await asClient(page, testInfo, "guest-delete");
    await page.goto("/stocks");
    const item = page.getByTestId("stock").filter({ hasText: stockName });
    await item.getByRole("button", { name: `${stockName} 삭제` }).click();
    const confirm = page.getByRole("alertdialog", {
      name: new RegExp(stockName),
    });
    await expect(confirm).toBeVisible();
    await confirm.getByRole("button", { name: "삭제" }).click();
    const unlock = page.getByRole("alertdialog", { name: "비밀번호 확인" });
    await expect(unlock).toBeVisible();
    await expect(confirm.getByRole("button", { name: "삭제 중…" })).toHaveCount(
      0,
    );
    // 비밀번호 창을 취소해도 삭제 확인창은 그대로 열려 있고, 종목은 지워지지 않는다.
    await unlock.getByRole("button", { name: "취소" }).click();
    await expect(unlock).toBeHidden();
    await expect(confirm).toBeVisible();
    await expect(item).toBeVisible();
  });

  test("손님의 읽음은 이 브라우저에만 남고 서버 읽음은 바뀌지 않는다", async ({
    page,
    browser,
    context,
  }, testInfo) => {
    await waitForListings();
    const { stockName, title, articleId } = seedGuestFixtures(
      testInfo.project.name,
    );
    await context.route("https://guest.example/**", (route) =>
      route.fulfill({ status: 200, contentType: "text/html", body: "원문" }),
    );
    await asClient(page, testInfo, "guest-read");
    await page.goto("/");
    await page
      .getByTestId("timeline-filters")
      .getByRole("navigation", { name: "항목 필터" })
      .getByRole("link", { name: stockName, exact: true })
      .click();
    const card = cards(page).filter({ hasText: title });
    await expect(card).toHaveAttribute("data-read", "false");

    // 링크를 누르면 읽음이 된다.
    const popup = context.waitForEvent("page");
    await card.getByRole("link", { name: title }).click();
    await (await popup).close();
    await expect(card).toHaveAttribute("data-read", "true");
    await page.reload();
    await expect(card).toHaveAttribute("data-read", "true");

    // 버튼으로 되돌리고 다시 표시해도 새로 고침 뒤에 남는다.
    await card.getByTestId("read-toggle").click();
    await expect(card).toHaveAttribute("data-read", "false");
    await page.reload();
    await expect(card).toHaveAttribute("data-read", "false");
    await card.getByTestId("read-toggle").click();
    await expect(card).toHaveAttribute("data-read", "true");
    await page.reload();
    await expect(card).toHaveAttribute("data-read", "true");

    // 서버 읽음은 그대로다.
    const db = new Database(E2E_DB_PATH, { readonly: true, timeout: 10_000 });
    try {
      const row = db
        .prepare<[number], { n: number }>(
          "select count(*) as n from article_reads where article_id = ?",
        )
        .get(articleId)!;
      expect(row.n).toBe(0);
    } finally {
      db.close();
    }

    // 주인으로 로그인한 다른 브라우저 문맥에서는 안 읽음이다.
    const other = await browser.newContext({
      baseURL: testInfo.project.use.baseURL,
      ...(testInfo.project.use.userAgent
        ? { userAgent: testInfo.project.use.userAgent }
        : {}),
    });
    try {
      const ownerPage = await other.newPage();
      await asClient(ownerPage, testInfo, "guest-read-owner");
      await login(ownerPage, "/");
      await ownerPage
        .getByTestId("timeline-filters")
        .getByRole("navigation", { name: "항목 필터" })
        .getByRole("link", { name: stockName, exact: true })
        .click();
      await expect(cards(ownerPage).filter({ hasText: title })).toHaveAttribute(
        "data-read",
        "false",
      );
    } finally {
      await other.close();
    }
  });

  test("손님도 기사 성격 탭과 제목 앞 태그를 같게 본다", async ({
    page,
  }, testInfo) => {
    await waitForListings();
    const { stockName, photoTitle, forecastTitle, factTitle } =
      seedNatureFixtures(testInfo.project.name);
    await asClient(page, testInfo, "guest-nature");
    await page.goto("/");
    await expect(page.locator("body")).toHaveAttribute("data-access", "guest");
    await page
      .getByRole("navigation", { name: "항목 필터" })
      .getByRole("link", { name: stockName, exact: true })
      .click();
    await expect(cards(page)).toHaveCount(1);
    await expect(
      cards(page).filter({ hasText: factTitle }).getByTestId("nature-tag"),
    ).toHaveText(["사실"]);
    await expect(page.getByText(photoTitle)).toHaveCount(0);
    await page
      .getByTestId("nature-tabs")
      .getByRole("link", { name: "해석·전망·기타", exact: true })
      .click();
    await expect(page).toHaveURL(/nature=view/);
    await expect(cards(page)).toHaveCount(1);
    await expect(
      cards(page).filter({ hasText: forecastTitle }).getByTestId("nature-tag"),
    ).toHaveText(["전망"]);
    await expect(page.getByText(photoTitle)).toHaveCount(0);
  });
});
