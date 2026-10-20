import { describe, expect, it } from "vitest";
import { clientKey, isHttpsRequest, safeNextPath } from "./request";

const headers = (values: Record<string, string>) => new Headers(values);

describe("clientKey", () => {
  it("Cloudflare가 넣은 접속자 IP를 쓴다", () => {
    expect(clientKey(headers({ "cf-connecting-ip": "203.0.113.7" }))).toBe(
      "ip:203.0.113.7",
    );
  });

  it("터널을 거치지 않은 로컬 접속은 하나로 묶고, X-Forwarded-For는 믿지 않는다", () => {
    expect(clientKey(headers({}))).toBe("direct");
    expect(clientKey(headers({ "x-forwarded-for": "198.51.100.1" }))).toBe(
      "direct",
    );
  });
});

describe("isHttpsRequest", () => {
  it("x-forwarded-proto의 첫 값이 https일 때만 참", () => {
    expect(isHttpsRequest(headers({ "x-forwarded-proto": "https" }))).toBe(
      true,
    );
    expect(
      isHttpsRequest(headers({ "x-forwarded-proto": "https, http" })),
    ).toBe(true);
    expect(isHttpsRequest(headers({ "x-forwarded-proto": "http" }))).toBe(
      false,
    );
    expect(isHttpsRequest(headers({}))).toBe(false);
  });
});

describe("safeNextPath (로그인 후 이동할 곳)", () => {
  it.each([
    ["/", "/"],
    ["/stocks", "/stocks"],
    ["/stocks?market=KR#top", "/stocks?market=KR#top"],
  ])("같은 사이트 경로는 그대로: %s", (value, expected) => {
    expect(safeNextPath(value)).toBe(expected);
  });

  it.each([
    undefined,
    null,
    "",
    "stocks",
    "https://evil.example",
    "//evil.example",
    "/\\evil.example",
    "/\t/evil.example",
    "javascript:alert(1)",
    "/login",
    "/login?next=/",
  ])("외부 주소·이상한 값·로그인 화면이면 홈으로: %s", (value) => {
    expect(safeNextPath(value)).toBe("/");
  });
});
