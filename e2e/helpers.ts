import { expect, type Page, type TestInfo } from "@playwright/test";
import { E2E_APP_PASSWORD, E2E_PORTS } from "./test-env";

/**
 * 테스트마다 다른 접속자 IP로 보이게 한다. 로그인 실패 제한(IP별)이 테스트끼리 섞이지 않게 하기 위함.
 * 실제로는 Cloudflare가 이 헤더를 넣는다.
 */
export async function asClient(page: Page, testInfo: TestInfo, label: string) {
  await page.setExtraHTTPHeaders({
    "cf-connecting-ip": `e2e-${testInfo.project.name}-${label}`,
  });
}

/** 로그인 화면에서 비밀번호를 내고, 서버 응답을 기다린다. */
export async function submitPassword(page: Page, password: string) {
  await page.getByLabel("비밀번호").fill(password);
  await Promise.all([
    page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        new URL(response.url()).pathname === "/login",
    ),
    page.getByRole("button", { name: "로그인" }).click(),
  ]);
}

export async function login(page: Page, path = "/") {
  await page.goto(path);
  await expect(page).toHaveURL(/\/login/);
  await submitPassword(page, E2E_APP_PASSWORD);
  await expect(page).not.toHaveURL(/\/login/);
}

/** worker가 시작하면서 받는 공식 종목 목록이 준비될 때까지 기다린다(로컬 E2E 전용). */
export async function waitForListings() {
  await expect
    .poll(async () => {
      const response = await fetch(
        `http://127.0.0.1:${E2E_PORTS.worker}/health`,
      );
      const body = (await response.json()) as {
        listings?: { KR: number; US: number };
      };
      return (body.listings?.KR ?? 0) > 0 && (body.listings?.US ?? 0) > 0;
    })
    .toBe(true);
}

export async function searchStock(page: Page, query: string) {
  await page.getByLabel("종목 이름 또는 티커").fill(query);
  await page.getByRole("button", { name: "찾기" }).click();
}

/** 찾은 후보에서 종목을 등록하고, 등록한 종목 줄을 돌려준다. */
export async function registerStock(page: Page, query: string, name: string) {
  await searchStock(page, query);
  const candidate = page.getByTestId("candidate").filter({ hasText: name });
  await candidate.getByRole("button", { name: `${name} 등록` }).click();
  await expect(candidate).toContainText("등록함");
  return page.getByTestId("stock").filter({ hasText: name });
}
