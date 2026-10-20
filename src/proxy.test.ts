import { NextRequest } from "next/server";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { getAuthConfig } from "@/server/auth/config";
import { createSessionToken, SESSION_COOKIE } from "@/server/auth/session";
import { proxy } from "./proxy";

beforeAll(() => {
  vi.stubEnv("APP_PASSWORD", "proxy-test-password");
  vi.stubEnv("SESSION_SECRET", "proxy-test-session-secret-0123456789");
});

function request(path: string, cookie?: string) {
  return new NextRequest(new URL(path, "http://127.0.0.1:3000"), {
    headers: cookie ? { cookie: `${SESSION_COOKIE}=${cookie}` } : {},
  });
}

const passedThrough = (response: Response) =>
  response.headers.get("x-middleware-next") === "1";

/** 호스트는 Next가 응답 직전에 상대 경로로 바꾸므로 경로와 쿼리만 본다(E2E에서 호스트 유지를 확인). */
const redirectTarget = (response: Response) => {
  const url = new URL(response.headers.get("location")!);
  return `${url.pathname}${url.search}`;
};

describe("proxy: 미로그인 시 모든 페이지·API 차단 (SPEC FR-1)", () => {
  it("페이지는 로그인 화면으로 보내고, 가려던 경로를 next로 남긴다", async () => {
    const home = await proxy(request("/"));
    expect(home.status).toBe(307);
    expect(redirectTarget(home)).toBe("/login");

    const deep = await proxy(request("/stocks/123?tab=keywords"));
    expect(redirectTarget(deep)).toBe(
      "/login?next=%2Fstocks%2F123%3Ftab%3Dkeywords",
    );

    const homeWithQuery = await proxy(request("/?tab=status"));
    expect(redirectTarget(homeWithQuery)).toBe(
      "/login?next=%2F%3Ftab%3Dstatus",
    );
  });

  it("API는 401로 거절한다", async () => {
    const response = await proxy(request("/api/anything"));
    expect(response.status).toBe(401);
    expect(await response.json()).toEqual({ error: "unauthorized" });
  });

  it("잘못된 세션 쿠키도 막는다", async () => {
    const response = await proxy(request("/", "forged-token"));
    expect(response.status).toBe(307);
  });

  it("로그인 화면은 그대로 통과시킨다", async () => {
    expect(passedThrough(await proxy(request("/login")))).toBe(true);
  });

  it("유효한 세션이면 페이지·API 모두 통과시킨다", async () => {
    const token = await createSessionToken(getAuthConfig());
    expect(passedThrough(await proxy(request("/", token)))).toBe(true);
    expect(passedThrough(await proxy(request("/api/anything", token)))).toBe(
      true,
    );
  });
});
