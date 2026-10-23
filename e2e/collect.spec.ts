import { expect, test } from "@playwright/test";
import { asClient, login, registerStock, waitForListings } from "./helpers";

// 가짜 언론사 RSS·Finnhub·Jev가 필요한 테스트라 로컬에서만 돈다.
test.skip(
  Boolean(process.env.E2E_BASE_URL),
  "가짜 외부 서비스가 필요한 테스트라 로컬 E2E에서만 실행한다",
);

test.describe("기사 수집과 평가 (SPEC FR-4, FR-5)", () => {
  test("상태 화면에서 '지금 수집'을 누르면 종목에 걸린 기사를 모아 Jev로 평가하고 출처별 결과를 보여 준다", async ({
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

    await page.goto("/status");
    const panel = page.getByTestId("collection");
    await expect(panel).toBeVisible();
    // 다음 정기 수집과 키워드 재생성 시각(6단계)
    const schedule = panel.getByTestId("collection-schedule");
    await expect(schedule).toContainText("정기 수집");
    await expect(schedule).toContainText("30분마다");
    await expect(schedule).toContainText(/매일 06:00 · (곧 시작|다음 )/);
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
    // 가짜 Jev는 종목 이름으로 걸린 기사(SK하이닉스 2건)에 관련 8점·영향 7점, 키워드로 걸린 기사에 3점을 준다.
    const evaluation = run.getByTestId("collection-evaluation");
    await expect(evaluation).toContainText(
      /평가 [1-9]\d*건.* · 표시 기준 이상 [1-9]\d*건/,
    );
    await expect(evaluation).not.toContainText("실패");
    await expect(panel).toContainText(
      /평가 [1-9]\d*건\(표시 기준 이상 [1-9]\d*건\)/,
    );
    // "지금 수집"도 간격 계산에 넣어, 다음 정기 수집은 30분 뒤다.
    await expect(schedule).toContainText(
      /30분마다 · 다음 (\d{2}-\d{2} )?\d{2}:\d{2}/,
    );

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
    // 좁은 화면(320px)에서 줄이 바뀌는 모양을 캡처해 본다.
    await page.setViewportSize({ width: 320, height: 900 });
    await panel.screenshot({
      path: testInfo.outputPath("collect-narrow.png"),
      animations: "disabled",
    });
    await page.setViewportSize({ width: 1280, height: 900 });

    // 11단계: 모은 제목으로 시장 토픽을 갱신하고(가짜 AI가 토픽 2개와 관련 종목 SK하이닉스를 냄),
    // 지난 기사를 토픽에 걸어 다음 수집에서 평가한 뒤 타임라인 맨 위 토픽 카드로 본다.
    const topics = page.getByTestId("topic-refresh");
    await expect(topics).toContainText("3시간마다");
    await topics.getByRole("button", { name: "지금 갱신" }).click();
    // 가짜 AI의 토픽 2개(타임라인 시험이 먼저 넣은 '중동 긴장'이 있으면 이어져 3개)
    await expect(topics).toContainText(/진행 중인 토픽 [23]개/, {
      timeout: 15_000,
    });
    await expect(topics).toContainText(/마지막 갱신 \d{2}-\d{2} \d{2}:\d{2}/);
    // 설정 화면 시험이 특정 시각을 잠시 넣을 수 있어 그 몫은 열어 둔다.
    await expect(topics).toContainText(/3시간마다.* · 다음 /);
    await page.screenshot({
      path: testInfo.outputPath("topics-refreshed.png"),
      fullPage: true,
      animations: "disabled",
    });
    // 지난 기사를 건 연결은 다음 수집 회차에서 평가한다.
    await panel.getByRole("button", { name: "지금 수집" }).click();
    await expect(panel.getByRole("button", { name: "수집 중…" })).toBeVisible();
    await expect(panel.getByRole("button", { name: "지금 수집" })).toBeEnabled({
      timeout: 30_000,
    });
    await expect(run).toContainText("완료");

    await page.goto("/");
    const card = page
      .getByTestId("topic-card")
      .filter({ hasText: "AI 투자 확대" });
    await expect(card).toContainText("빅테크가 데이터센터 투자를 늘림");
    await expect(card).toContainText(/기사 [1-9]\d*건/);
    await expect(card.getByTestId("topic-related")).toContainText("SK하이닉스");
    // 기사가 걸리지 않은 토픽은 카드가 없다.
    await expect(
      page.getByTestId("topic-card").filter({ hasText: "미국 금리 인하 기대" }),
    ).toHaveCount(0);
    await page.screenshot({
      path: testInfo.outputPath("timeline-topics.png"),
      fullPage: true,
      animations: "disabled",
    });

    // 카드를 누르면 그 토픽 기사만, 한 번 더 누르면 풀린다.
    await card.click();
    await expect(page).toHaveURL(/[?&]topic=\d+/);
    await expect(page.getByTestId("topic-filter")).toContainText(
      "토픽 AI 투자 확대 기사만 보는 중",
    );
    const shown = page.getByTestId("timeline-card");
    await expect(shown.first()).toContainText(
      "AI 반도체 수요 급증에 데이터센터 투자 확대",
    );
    await expect(
      shown
        .first()
        .getByTestId("timeline-stock")
        .filter({ hasText: "주제" })
        .first(),
    ).toContainText(/AI 투자 확대\s*관련도 8\.0 · 영향도 7\.0/);
    await page.getByRole("link", { name: "모두 보기" }).click();
    await expect(page).not.toHaveURL(/topic=/);
  });
});
