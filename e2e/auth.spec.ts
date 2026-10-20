import { expect, test } from "@playwright/test";
import { asClient, login, submitPassword } from "./helpers";
import { E2E_APP_PASSWORD } from "./test-env";

test.describe("로그인 (SPEC FR-1, D-10)", () => {
  test("미로그인으로 홈에 오면 로그인 화면으로 보낸다", async ({
    page,
  }, testInfo) => {
    await asClient(page, testInfo, "redirect");
    await page.goto("/");

    await expect(page).toHaveURL(/\/login$/);
    // 접속한 호스트를 유지한다(127.0.0.1이 localhost로 바뀌면 쿠키가 따로 놀게 된다).
    expect(new URL(page.url()).host).toBe(
      new URL(testInfo.project.use.baseURL!).host,
    );
    await expect(page.getByRole("heading", { name: "Ripple" })).toBeVisible();
    await expect(page.getByLabel("비밀번호")).toBeVisible();
    await page.screenshot({
      path: testInfo.outputPath("login.png"),
      fullPage: true,
    });
  });

  test("미로그인으로 다른 페이지·API에 와도 막는다", async ({
    page,
    request,
  }) => {
    await page.goto("/some/page?x=1");
    await expect(page).toHaveURL(/\/login\?next=%2Fsome%2Fpage%3Fx%3D1$/);

    const api = await request.get("/api/anything");
    expect(api.status()).toBe(401);
    expect(await api.json()).toEqual({ error: "unauthorized" });
  });

  test("틀린 비밀번호는 오류를 보여 준다", async ({ page }, testInfo) => {
    await asClient(page, testInfo, "wrong-password");
    await page.goto("/login");
    await submitPassword(page, "wrong-password-123");

    await expect(page.getByRole("main").getByRole("alert")).toHaveText(
      "비밀번호가 맞지 않습니다.",
    );
    await expect(page).toHaveURL(/\/login$/);
    await page.screenshot({
      path: testInfo.outputPath("login-error.png"),
      fullPage: true,
    });
  });

  test("로그인하면 가려던 페이지로 가고, 로그아웃하면 다시 막힌다", async ({
    page,
  }, testInfo) => {
    await asClient(page, testInfo, "login-logout");
    await login(page, "/?tab=status");

    await expect(page).toHaveURL(/\/\?tab=status$/);
    await expect(page.getByRole("button", { name: "로그아웃" })).toBeVisible();
    await expect(page.getByTestId("db-status")).toBeVisible();

    await page.getByRole("button", { name: "로그아웃" }).click();
    await expect(page).toHaveURL(/\/login$/);
    await page.goto("/");
    await expect(page).toHaveURL(/\/login$/);
  });

  test("이미 로그인했으면 로그인 화면 대신 가려던 곳으로 보낸다", async ({
    page,
  }, testInfo) => {
    await asClient(page, testInfo, "already-logged-in");
    await login(page);
    await page.goto("/login?next=%2F%3Fagain%3D1");
    await expect(page).toHaveURL(/\/\?again=1$/);
  });

  test("5번 틀리면 15분 동안 맞는 비밀번호도 받지 않는다", async ({
    page,
  }, testInfo) => {
    await asClient(page, testInfo, "rate-limit");
    await page.goto("/login");
    const alert = page.getByRole("main").getByRole("alert");

    for (let i = 1; i <= 4; i++) {
      await submitPassword(page, `wrong-password-${i}`);
      await expect(alert).toHaveText("비밀번호가 맞지 않습니다.");
    }
    await submitPassword(page, "wrong-password-5");
    await expect(alert).toHaveText(
      "로그인 시도가 너무 많습니다. 15분 뒤에 다시 시도하세요.",
    );

    await submitPassword(page, E2E_APP_PASSWORD);
    await expect(alert).toContainText("로그인 시도가 너무 많습니다.");
    await expect(page).toHaveURL(/\/login$/);
    await page.screenshot({
      path: testInfo.outputPath("login-blocked.png"),
      fullPage: true,
    });
  });

  test("세션 쿠키는 HttpOnly·SameSite=Lax·30일 만료", async ({
    page,
    context,
  }, testInfo) => {
    await asClient(page, testInfo, "cookie");
    await login(page);

    const cookie = (await context.cookies()).find(
      (c) => c.name === "ripple_session",
    );
    expect(cookie).toBeDefined();
    expect(cookie!.httpOnly).toBe(true);
    expect(cookie!.sameSite).toBe("Lax");
    const days = (cookie!.expires * 1000 - Date.now()) / (24 * 60 * 60 * 1000);
    expect(days).toBeGreaterThan(29.9);
    expect(days).toBeLessThanOrEqual(30);
  });
});
