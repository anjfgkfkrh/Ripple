// 14단계 R10 임시 시험(R12에서 지움): collect.spec은 종목 화면(R12)·타임라인(R11)이 필요해 Rust web에서 돌 수 없으므로,
// 종목은 worker API로 직접 넣고 상태 화면의 수집·토픽 패널만 확인한다. DECISIONS.md 1장 "14단계 R10 결정" ①
import { expect, test } from "@playwright/test";
import { asClient, login, waitForListings } from "./helpers";
import { E2E_PORTS, E2E_WORKER_API_TOKEN } from "./test-env";

test("상태 화면에서 '지금 수집'을 누르면 기사를 모아 평가하고 출처별 결과를 보여 준다", async ({
  page,
}, testInfo) => {
  test.skip(
    testInfo.project.name !== "desktop",
    "수집은 앱 전체에 하나라서 한 번만 돈다",
  );
  await waitForListings();
  const worker = `http://127.0.0.1:${E2E_PORTS.worker}`;
  const response = await fetch(`${worker}/stocks`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${E2E_WORKER_API_TOKEN}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      market: "KR",
      code: "000660",
      userInput: "하닉",
      nameKo: "SK하이닉스",
    }),
  });
  expect([200, 201, 409]).toContain(response.status);

  await asClient(page, testInfo, "status-r10");
  await login(page, "/status");
  const panel = page.getByTestId("collection");
  await expect(panel).toBeVisible();
  const schedule = panel.getByTestId("collection-schedule");
  await expect(schedule).toContainText("정기 수집");
  await expect(schedule).toContainText("30분마다");
  await expect(schedule).toContainText(/매일 06:00 · (곧 시작|다음 )/);
  await panel.getByRole("button", { name: "지금 수집" }).click();
  await expect(panel.getByRole("button", { name: "수집 중…" })).toBeVisible();

  const run = page.getByTestId("collection-run");
  await expect(run).toContainText("완료", { timeout: 30_000 });
  await expect(panel.getByRole("button", { name: "지금 수집" })).toBeEnabled();
  await expect(run).toContainText(/새 기사 [1-9]\d*건/);
  await expect(panel).not.toContainText("저장된 기사 0건");
  const evaluation = run.getByTestId("collection-evaluation");
  await expect(evaluation).toContainText(
    /평가 [1-9]\d*건.* · 표시 기준 이상 [1-9]\d*건/,
  );
  await expect(evaluation).not.toContainText("실패");
  await expect(schedule).toContainText(
    /30분마다 · 다음 (\d{2}-\d{2} )?\d{2}:\d{2}/,
  );

  await run.getByText(/출처별 결과 \d+곳/).click();
  const sources = run.getByTestId("collection-sources");
  await expect(sources).toContainText("가짜경제");
  await expect(sources).toContainText("가짜뉴스");
  await expect(sources).toContainText("Finnhub");

  const topics = page.getByTestId("topic-refresh");
  await expect(topics).toContainText("3시간마다");
  await topics.getByRole("button", { name: "지금 갱신" }).click();
  await expect(topics).toContainText(/진행 중인 토픽 [1-9]\d*개/, {
    timeout: 15_000,
  });
  await expect(topics).toContainText(/마지막 갱신 \d{2}-\d{2} \d{2}:\d{2}/);
});
