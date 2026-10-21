import Database from "better-sqlite3";
import { expect, test, type Locator } from "@playwright/test";
import { asClient, login, waitForListings } from "./helpers";
import { E2E_DB_PATH } from "./test-env";

// 테스트 DB에 기사·평가를 직접 넣으므로 로컬 E2E에서만 돈다(수집·평가 흐름은 collect.spec이 검사).
test.skip(
  Boolean(process.env.E2E_BASE_URL),
  "테스트 DB에 직접 넣는 테스트라 로컬 E2E에서만 실행한다",
);

const MIN = 60 * 1000;

/**
 * 다른 테스트가 쓰지 않는 종목 둘(멜콘, 나노솔루션)과 기사·평가를 넣는다. 데스크톱·모바일이 함께 돌아도
 * 한 번만 넣도록 쓰기 잠금을 잡고 확인한다.
 */
function seedTimeline(): { melcon: number; nano: number } {
  const db = new Database(E2E_DB_PATH, { timeout: 10_000 });
  try {
    return db
      .transaction(() => {
        const find = db.prepare<[string], { id: number }>(
          "select id from stocks where market = 'KR' and code = ?",
        );
        const existing = find.get("179880");
        if (existing) {
          return { melcon: existing.id, nano: find.get("238540")!.id };
        }
        const now = Date.now();
        const insertStock = db.prepare(
          `insert into stocks (market, code, name, name_ko, exchange, user_input, created_at, keywords_status, keywords_generated_at)
           values ('KR', ?, ?, ?, ?, ?, ?, 'ready', ?)`,
        );
        const stockId = (code: string, name: string, exchange: string) =>
          Number(
            insertStock.run(code, name, name, exchange, name, now, now)
              .lastInsertRowid,
          );
        const melcon = stockId("179880", "멜콘", "KOSDAQ");
        const nano = stockId("238540", "나노솔루션", "KONEX");

        let serial = 0;
        const insertArticle = db.prepare(
          `insert into articles (url_key, url, title, publisher, feed, published_at, collected_at)
           values (?, ?, ?, ?, 'rss:mock:economy', ?, ?)`,
        );
        const insertMatch = db.prepare(
          `insert into article_matches (article_id, stock_id, keyword_path, match_kind, level, created_at)
           values (?, ?, ?, 'name', 0, ?)`,
        );
        const insertEvaluation = db.prepare(
          `insert into evaluations (match_id, article_id, stock_id, relevance, relevance_confidence, impact, impact_confidence, impact_probabilities, model, evaluated_at)
           values (?, ?, ?, ?, 0.7, ?, 0.8, '{}', 'jev-1.13.0', ?)`,
        );
        const article = (
          title: string,
          minutesAgo: number,
          links: [stock: number, relevance: number, impact: string][],
          publisher = "가짜경제",
        ) => {
          serial += 1;
          const at = now - minutesAgo * MIN;
          const articleId = Number(
            insertArticle.run(
              `timeline.example/${serial}`,
              `https://timeline.example/${serial}`,
              title,
              publisher,
              at,
              now,
            ).lastInsertRowid,
          );
          for (const [stock, relevance, impact] of links) {
            const matchId = Number(
              insertMatch.run(articleId, stock, "멜콘", now).lastInsertRowid,
            );
            insertEvaluation.run(
              matchId,
              articleId,
              stock,
              relevance,
              impact,
              now,
            );
          }
        };

        article("멜콘 신제품 공개", 2, [[melcon, 8, "positive"]]);
        // 제목이 같은 다른 언론사 기사 → 한 카드로 묶인다.
        article("멜콘 신제품 공개", 7, [[melcon, 8, "positive"]], "가짜일보");
        // 두 종목에 걸린 기사 → 카드 하나에 두 종목(D-26)
        article("멜콘·나노솔루션 합작", 3, [
          [melcon, 7, "positive"],
          [nano, 8.5, "negative"],
        ]);
        article("멜콘 공장 화재", 5, [[melcon, 9, "negative"]]);
        article("멜콘 주주총회", 15, [[melcon, 6.5, "neutral"]]);
        // 기준 미만은 보이지 않는다.
        article("멜콘 잡담", 4, [[melcon, 3, "neutral"]]);
        // '더 보기'용: 모두 합쳐 30개가 넘게
        for (let i = 1; i <= 30; i += 1) {
          article(`멜콘 소식 ${i}`, 20 + i * 10, [[melcon, 7, "positive"]]);
        }
        return { melcon, nano };
      })
      .immediate();
  } finally {
    db.close();
  }
}

const cards = (page: import("@playwright/test").Page) =>
  page.getByTestId("timeline-card");
const chip = (filters: Locator, group: string, name: string) =>
  filters
    .getByRole("navigation", { name: `${group} 필터` })
    .getByRole("link", { name, exact: true });

test.describe("타임라인 (SPEC FR-7, D-25, D-26)", () => {
  test("종목·영향 필터, 여러 종목 카드, 같은 제목 묶기, 더 보기", async ({
    page,
  }, testInfo) => {
    await waitForListings();
    const ids = seedTimeline();
    await asClient(page, testInfo, "timeline");
    await login(page, "/");
    await expect(page.getByRole("heading", { name: "타임라인" })).toBeVisible();
    const filters = page.getByTestId("timeline-filters");

    // 종목 필터: 주소에 남고, 30개 + '더 보기'
    await chip(filters, "종목", "멜콘").click();
    await expect(page).toHaveURL(new RegExp(`[?&]stock=${ids.melcon}`));
    await expect(cards(page)).toHaveCount(30);
    await expect(cards(page).first()).toContainText("멜콘 신제품 공개");
    await expect(page.getByText("멜콘 잡담")).toHaveCount(0);

    // 같은 제목은 한 카드로, 나머지 언론사는 '외 N곳'
    const product = cards(page).filter({ hasText: "멜콘 신제품 공개" });
    await expect(product).toHaveCount(1);
    await expect(product).toContainText("외 1곳: 가짜일보");
    await expect(
      product.getByRole("link", { name: /멜콘 신제품 공개/ }),
    ).toHaveAttribute("target", "_blank");

    // 두 종목에 걸린 기사는 카드 하나에 관련도 높은 종목부터
    const joint = cards(page).filter({ hasText: "멜콘·나노솔루션 합작" });
    await expect(joint.getByTestId("timeline-stock")).toHaveText([
      /악재\s*나노솔루션\s*관련도 8\.5/,
      /호재\s*멜콘\s*관련도 7\.0/,
    ]);
    await page.screenshot({
      path: testInfo.outputPath("timeline.png"),
      fullPage: true,
      animations: "disabled",
    });

    await page.getByRole("link", { name: "더 보기" }).click();
    await expect(page).toHaveURL(/limit=60/);
    await expect(cards(page)).toHaveCount(34);
    await expect(page.getByRole("link", { name: "더 보기" })).toHaveCount(0);

    // 영향 필터(종목과 함께면 같은 종목): 멜콘의 악재만
    await chip(filters, "영향", "악재").click();
    await expect(page).toHaveURL(/impact=negative/);
    await expect(page).not.toHaveURL(/limit=/);
    await expect(cards(page)).toHaveCount(1);
    await expect(cards(page)).toContainText("멜콘 공장 화재");

    // 종목을 '전체'로: 악재인 종목이 하나라도 있는 기사(나노솔루션 악재인 합작 기사 포함)
    await chip(filters, "종목", "전체").click();
    await expect(
      cards(page).filter({ hasText: "멜콘·나노솔루션 합작" }),
    ).toHaveCount(1);
    await expect(cards(page).filter({ hasText: "멜콘 공장 화재" })).toHaveCount(
      1,
    );
    await expect(chip(filters, "영향", "악재")).toHaveAttribute(
      "aria-current",
      "true",
    );

    // 뒤로 가기에도 고른 필터가 남는다.
    await page.goBack();
    await expect(page).toHaveURL(
      new RegExp(`stock=${ids.melcon}.*impact=negative`),
    );
    await expect(cards(page)).toHaveCount(1);
    await page.screenshot({
      path: testInfo.outputPath("timeline-filtered.png"),
      fullPage: true,
      animations: "disabled",
    });
  });
});
