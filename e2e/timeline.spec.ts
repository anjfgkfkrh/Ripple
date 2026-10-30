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

/** 서로 겹치는 두 글자가 거의 없는 제목(비슷한 제목 묶기에 걸리지 않게, 11단계) */
const distinct = (i: number) =>
  `멜콘 ${String.fromCharCode(0xac00 + i * 97)}${String.fromCharCode(0xac00 + i * 97 + 41)} 소식`;

/**
 * 다른 테스트가 쓰지 않는 종목 둘(멜콘, 나노솔루션)과 기사·평가, 자동 토픽 둘(중동 긴장, 옛 이슈)을 넣는다.
 * 데스크톱·모바일이 함께 돌아도 한 번만 넣도록 쓰기 잠금을 잡고 확인한다.
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
        // 주가(13단계): 멜콘은 오르고 나노솔루션은 내린다. 멜콘은 앞 값보다 새 값이 보여야 한다.
        const insertPrice = db.prepare(
          `insert into stock_prices (stock_id, price, change, change_percent, source, fetched_at)
           values (?, ?, ?, ?, 'kis', ?)`,
        );
        insertPrice.run(melcon, 15000, 0, 0, now - 2 * MIN);
        insertPrice.run(melcon, 15230, 230, 1.53, now - MIN);
        insertPrice.run(nano, 4120, -55, -1.32, now - MIN);

        let serial = 0;
        const insertArticle = db.prepare(
          `insert into articles (url_key, url, title, publisher, feed, published_at, collected_at)
           values (?, ?, ?, ?, 'rss:mock:economy', ?, ?)`,
        );
        const insertMatch = db.prepare(
          `insert into article_matches (article_id, stock_id, keyword_path, match_kind, level, created_at)
           values (?, ?, ?, ?, ?, ?)`,
        );
        // 영향도(11단계)를 주지 않으면 11단계 전 평가처럼 비워 관련도만 본다.
        const insertEvaluation = db.prepare(
          `insert into evaluations (match_id, article_id, stock_id, relevance, relevance_confidence, magnitude, impact, impact_confidence, impact_probabilities, model, evaluated_at)
           values (?, ?, ?, ?, 0.7, ?, ?, 0.8, '{}', 'jev-1.13.0', ?)`,
        );
        const article = (
          title: string,
          minutesAgo: number,
          links: [
            stock: number,
            relevance: number,
            impact: string,
            magnitude?: number,
            path?: string,
          ][],
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
          for (const [stock, relevance, impact, magnitude, path] of links) {
            const matchId = Number(
              insertMatch.run(
                articleId,
                stock,
                path ?? "멜콘",
                path ? "keyword" : "name",
                path ? 1 : 0,
                now,
              ).lastInsertRowid,
            );
            insertEvaluation.run(
              matchId,
              articleId,
              stock,
              relevance,
              magnitude ?? null,
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
        article("멜콘 공장 화재", 5, [[melcon, 9, "negative", 8]]);
        article("멜콘 주주총회", 15, [[melcon, 6.5, "neutral"]]);
        // 기준 미만은 보이지 않는다(관련도, 11단계부터는 영향도도).
        article("멜콘 잡담", 4, [[melcon, 3, "neutral"]]);
        article("멜콘 사옥 이전", 6, [[melcon, 8, "neutral", 4]]);
        // '더 보기'용: 모두 합쳐 30개가 넘게
        for (let i = 1; i <= 30; i += 1) {
          article(distinct(i), 20 + i * 10, [[melcon, 7, "positive"]]);
        }

        // 자동 토픽(11단계): 토픽과 한국 증시 기준 항목, 그 항목에 걸린 기사. 멜콘에는 토픽 가지를 붙인다.
        const topic = (name: string, summary: string, term: string) => {
          const topicId = Number(
            db
              .prepare(
                `insert into topics (source, name, summary, terms, status, created_at, last_seen_at)
                 values ('auto', ?, ?, ?, 'active', ?, ?)`,
              )
              .run(name, summary, JSON.stringify([term]), now, now)
              .lastInsertRowid,
          );
          const lens = Number(
            db
              .prepare(
                `insert into stocks (market, code, name, name_ko, exchange, user_input, created_at, keywords_status, keywords_generated_at, kind, topic_id)
                 values ('KR', ?, ?, ?, '한국 증시', ?, ?, 'ready', ?, 'topic', ?)`,
              )
              .run(`TOPIC-${topicId}`, name, name, name, now, now, topicId)
              .lastInsertRowid,
          );
          return { topicId, lens };
        };
        const mideast = topic("중동 긴장", "사우디 시설 공격 위협", "사우디");
        article("사우디 원유 시설 피격", 30, [
          [mideast.lens, 8, "negative", 7, "중동 긴장 → 사우디"],
        ]);
        article("사우디 증산 합의 불발", 90, [
          [mideast.lens, 7, "negative", 6.5, "중동 긴장 → 사우디"],
          [melcon, 7, "positive", 6, "멜콘 → 중동 긴장 → 사우디"],
        ]);
        db.prepare(
          `insert into keywords (stock_id, level, text_ko, reason, path, kind, relevance, magnitude, topic_id, created_at)
           values (?, 1, '중동 긴장', '중동 수주', '멜콘 → 중동 긴장', 'current', 7, 6, ?, ?)`,
        ).run(melcon, mideast.topicId, now);
        // 12시간 넘게 새 기사가 없는 토픽은 '이전 이슈'로 접는다.
        const old = topic("옛 이슈", "지난 소식", "옛날");
        article("옛날 옛적 소식", 13 * 60, [
          [old.lens, 8, "neutral", 7, "옛 이슈 → 옛날"],
        ]);
        return { melcon, nano };
      })
      .immediate();
  } finally {
    db.close();
  }
}

/**
 * 읽음 표시(12단계) 시험용으로 프로젝트(데스크톱·모바일)마다 따로 종목 하나와 기사를 넣는다.
 * 읽음 상태가 DB 하나를 같이 쓰므로, 두 프로젝트가 같은 기사를 건드리지 않게 하기 위함이다.
 * 기사는 6시간 전이라 다른 시험의 맨 위 30개를 밀어내지 않고, 이 종목 칩으로만 본다.
 */
function seedReadFixtures(project: string): {
  stockName: string;
  groupTitle: string;
  loneTitle: string;
} {
  const db = new Database(E2E_DB_PATH, { timeout: 10_000 });
  const stockName = `읽음시험${project}`;
  const groupTitle = `${project} 기준금리 동결 연 2.5% 유지`;
  const loneTitle = `${project} 반도체 수출 호조 이어져`;
  try {
    db.transaction(() => {
      const code = `READ-${project}`;
      if (
        db
          .prepare("select id from stocks where market = 'KR' and code = ?")
          .get(code)
      ) {
        return;
      }
      const now = Date.now();
      const stock = Number(
        db
          .prepare(
            `insert into stocks (market, code, name, name_ko, exchange, user_input, created_at, keywords_status, keywords_generated_at)
             values ('KR', ?, ?, ?, 'KOSDAQ', ?, ?, 'ready', ?)`,
          )
          .run(code, stockName, stockName, stockName, now, now).lastInsertRowid,
      );
      const article = (title: string, hoursAgo: number, publisher: string) => {
        const at = now - hoursAgo * 60 * MIN;
        const key = `read.example/${project}/${hoursAgo}/${publisher}`;
        const articleId = Number(
          db
            .prepare(
              `insert into articles (url_key, url, title, publisher, feed, published_at, collected_at)
               values (?, ?, ?, ?, 'rss:mock:economy', ?, ?)`,
            )
            .run(key, `https://${key}`, title, publisher, at, now)
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
      };
      article(loneTitle, 6, "가짜경제");
      // 비슷한 제목의 다른 언론사 기사 → 한 카드로 묶인다.
      article(groupTitle, 7, "가짜경제");
      article(
        `${project} 한은 기준금리 동결 연 2.5% 유지 결정`,
        7.5,
        "가짜일보",
      );
    }).immediate();
  } finally {
    db.close();
  }
  return { stockName, groupTitle, loneTitle };
}

const cards = (page: import("@playwright/test").Page) =>
  page.getByTestId("timeline-card");
const chip = (filters: Locator, group: string, name: string) =>
  filters
    .getByRole("navigation", { name: `${group} 필터` })
    .getByRole("link", { name, exact: true });
/**
 * 읽음 저장(서버 액션 POST)이 끝날 때까지 기다린다. 화면은 낙관적으로 먼저 바뀌므로, 기다리지 않고
 * 새로 고치면 저장 전 DB로 그려져 간헐 실패했다(2026-10-30, HEAD에서도 10번 중 1번).
 */
const readSaved = (page: import("@playwright/test").Page) =>
  page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      response.request().headers()["next-action"] !== undefined,
  );

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
    await chip(filters, "항목", "멜콘").click();
    await expect(page).toHaveURL(new RegExp(`[?&]stock=${ids.melcon}`));
    await expect(cards(page)).toHaveCount(30);
    await expect(cards(page).first()).toContainText("멜콘 신제품 공개");
    await expect(page.getByText("멜콘 잡담")).toHaveCount(0);
    // 영향도가 기준 미만이면 관련도가 높아도 보이지 않는다(11단계).
    await expect(page.getByText("멜콘 사옥 이전")).toHaveCount(0);
    await expect(
      cards(page)
        .filter({ hasText: "멜콘 공장 화재" })
        .getByTestId("timeline-stock"),
    ).toHaveText(/악재\s*멜콘\s*관련도 9\.0 · 영향도 8\.0/);

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
    // 멜콘 카드 35개: 30개 + 신제품·합작·화재·주주총회 + 중동 토픽 가지로 걸린 기사
    await expect(cards(page)).toHaveCount(35);
    await expect(page.getByRole("link", { name: "더 보기" })).toHaveCount(0);

    // 영향 필터(종목과 함께면 같은 종목): 멜콘의 악재만
    await chip(filters, "영향", "악재").click();
    await expect(page).toHaveURL(/impact=negative/);
    await expect(page).not.toHaveURL(/limit=/);
    await expect(cards(page)).toHaveCount(1);
    await expect(cards(page)).toContainText("멜콘 공장 화재");

    // 종목을 '전체'로: 악재인 종목이 하나라도 있는 기사(나노솔루션 악재인 합작 기사 포함)
    await chip(filters, "항목", "전체").click();
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

  test("맨 위 주가 띠: 최근 값, 상승 빨강·하락 파랑, 누르면 그 종목 기사만(13단계)", async ({
    page,
  }, testInfo) => {
    seedTimeline();
    await login(page);
    await page.goto("/");
    const strip = page.getByTestId("price-strip");
    const melcon = strip.getByTestId("price-item").filter({ hasText: "멜콘" });
    const nano = strip
      .getByTestId("price-item")
      .filter({ hasText: "나노솔루션" });
    // 스트리밍된 결과는 숨겨진 채 DOM에 먼저 들어오므로 "보이는지"까지 기다린다.
    await expect(melcon).toBeVisible();
    await expect(melcon).toContainText("15,230원");
    await expect(melcon.getByTestId("price-change")).toHaveText("▲230 +1.53%");
    await expect(melcon.getByTestId("price-change")).toHaveClass(/text-red/);
    await expect(nano.getByTestId("price-change")).toHaveText("▼55 -1.32%");
    await expect(nano.getByTestId("price-change")).toHaveClass(/text-blue/);
    await expect(melcon).toContainText("기준");
    await page.screenshot({ path: testInfo.outputPath("price-strip.png") });

    await melcon.click();
    await expect(page).toHaveURL(/\?stock=\d+$/);
    await expect(melcon).toHaveAttribute("aria-current", "true");
    await expect(
      page
        .getByRole("navigation", { name: "항목 필터" })
        .getByRole("link", { name: "멜콘" }),
    ).toHaveAttribute("aria-current", "true");
    // 한 번 더 누르면 풀린다.
    await melcon.click();
    await expect(page).toHaveURL(/\/$/);
  });

  test("맨 위 '지금 시장 이슈' 토픽 카드: 누르면 그 토픽 기사만, 식은 토픽은 '이전 이슈'로 접는다(11단계)", async ({
    page,
  }, testInfo) => {
    await waitForListings();
    seedTimeline();
    await asClient(page, testInfo, "timeline-topics");
    await login(page, "/");
    const strip = page.getByTestId("topic-cards");
    await expect(strip).toContainText("지금 시장 이슈");
    const card = strip
      .getByTestId("topic-card")
      .filter({ hasText: "중동 긴장" });
    await expect(card).toContainText("사우디 시설 공격 위협");
    await expect(card).toContainText("기사 2건");
    // 기준 이상 기사들의 호재/악재 중 많은 쪽, 붙은 내 종목과 그 가지 기사의 호재/악재
    await expect(card).toContainText("악재");
    await expect(card.getByTestId("topic-related")).toHaveText(
      /관련 내 종목\s*멜콘\s*호재/,
    );
    // 자동 토픽은 필터 칩에 넣지 않는다.
    await expect(
      page
        .getByTestId("timeline-filters")
        .getByRole("link", { name: "중동 긴장" }),
    ).toHaveCount(0);
    const faded = strip.getByText(/이전 이슈 \d+개/);
    await expect(faded).toBeVisible();
    await expect(
      strip.getByTestId("topic-card").filter({ hasText: "옛 이슈" }),
    ).toBeHidden();
    await page.screenshot({
      path: testInfo.outputPath("timeline-topic-cards.png"),
      fullPage: true,
      animations: "disabled",
    });

    await card.click();
    await expect(page).toHaveURL(/[?&]topic=\d+/);
    await expect(card).toHaveAttribute("aria-current", "true");
    await expect(page.getByTestId("topic-filter")).toContainText(
      "토픽 중동 긴장 기사만 보는 중",
    );
    await expect(cards(page)).toHaveText([
      /사우디 원유 시설 피격/,
      /사우디 증산 합의 불발/,
    ]);
    // 주제 항목은 이름 앞에 '주제', 같은 기사의 종목도 함께 보인다(관련도가 같으면 먼저 등록한 항목부터).
    await expect(cards(page).nth(1).getByTestId("timeline-stock")).toHaveText([
      /호재\s*멜콘\s*관련도 7\.0 · 영향도 6\.0/,
      /악재\s*주제\s*중동 긴장\s*관련도 7\.0 · 영향도 6\.5/,
    ]);
    await page.screenshot({
      path: testInfo.outputPath("timeline-topic-filtered.png"),
      fullPage: true,
      animations: "disabled",
    });
    // 한 번 더 누르면 풀린다.
    await card.click();
    await expect(page).not.toHaveURL(/topic=/);

    await faded.click();
    await expect(
      strip.getByTestId("topic-card").filter({ hasText: "옛 이슈" }),
    ).toBeVisible();
  });
  test("읽은 기사 표시: 링크를 누르면 묶인 카드째 읽음, 버튼으로 되돌리고, 새로 고쳐도 남는다(12단계)", async ({
    page,
    context,
  }, testInfo) => {
    await waitForListings();
    const { stockName, groupTitle, loneTitle } = seedReadFixtures(
      testInfo.project.name,
    );
    // 원문 링크는 새 탭으로 열린다. 가짜 주소라 외부로 나가지 않게 막는다.
    await context.route("https://read.example/**", (route) =>
      route.fulfill({ status: 200, contentType: "text/html", body: "원문" }),
    );
    await asClient(page, testInfo, "timeline-read");
    await login(page, "/");
    await chip(page.getByTestId("timeline-filters"), "항목", stockName).click();
    await expect(cards(page)).toHaveCount(2);

    const lone = cards(page).filter({ hasText: loneTitle });
    const group = cards(page).filter({ hasText: groupTitle });
    await expect(group).toContainText("외 1곳: 가짜일보");
    await expect(lone).toHaveAttribute("data-read", "false");
    await expect(lone.getByTestId("read-label")).toHaveCount(0);
    await expect(lone.getByTestId("read-toggle")).toHaveText("읽음으로 표시");
    await page.screenshot({
      path: testInfo.outputPath("timeline-unread.png"),
      fullPage: true,
      animations: "disabled",
    });

    // 제목 링크를 누르면 새 탭이 열리고, 이 카드는 읽음이 된다. 다른 카드는 그대로다.
    const popup = context.waitForEvent("page");
    const loneSaved = readSaved(page);
    await lone.getByRole("link", { name: loneTitle }).click();
    await (await popup).close();
    await loneSaved;
    await expect(lone).toHaveAttribute("data-read", "true");
    await expect(lone.getByTestId("read-label")).toBeVisible();
    await expect(lone.getByTestId("read-toggle")).toHaveText("안 읽음으로");
    await expect(group).toHaveAttribute("data-read", "false");
    // 순서는 그대로다(읽은 카드를 내리지 않는다).
    await expect(cards(page).first()).toContainText(loneTitle);

    // 새로 고쳐도(서버에 저장돼) 읽음으로 남고, 모양이 다르다(흐린 제목).
    await page.reload();
    await expect(lone).toHaveAttribute("data-read", "true");
    await expect(group).toHaveAttribute("data-read", "false");
    await page.screenshot({
      path: testInfo.outputPath("timeline-read.png"),
      fullPage: true,
      animations: "disabled",
    });

    // 묶인 카드는 '외 N곳' 링크를 눌러도 카드 전체가 읽음이 된다. 버튼으로 되돌린다.
    const other = context.waitForEvent("page");
    const groupSaved = readSaved(page);
    await group.getByRole("link", { name: "가짜일보" }).click();
    await (await other).close();
    await groupSaved;
    await expect(group).toHaveAttribute("data-read", "true");
    await page.reload();
    await expect(group).toHaveAttribute("data-read", "true");
    const unreadSaved = readSaved(page);
    await group.getByTestId("read-toggle").click();
    await expect(group).toHaveAttribute("data-read", "false");
    await unreadSaved;
    await page.reload();
    await expect(group).toHaveAttribute("data-read", "false");
    await expect(lone).toHaveAttribute("data-read", "true");

    // 버튼으로 읽음 표시하면 링크를 열지 않아도 읽음이 된다.
    const readAgainSaved = readSaved(page);
    await group.getByTestId("read-toggle").click();
    await expect(group).toHaveAttribute("data-read", "true");
    await readAgainSaved;
    await page.reload();
    await expect(group).toHaveAttribute("data-read", "true");
  });
});
