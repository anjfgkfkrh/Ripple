import { describe, expect, it } from "vitest";
import type { HttpClient } from "./http";
import { createRobotsChecker, isAllowed, parseRobots } from "./robots";

describe("parseRobots·isAllowed (RFC 9309)", () => {
  it("'*' 그룹을 쓰고, 가장 길게 맞는 규칙을 따른다", () => {
    const rules = parseRobots(
      [
        "User-agent: Googlebot",
        "Disallow: /",
        "",
        "User-agent: *",
        "Disallow: /",
        "Allow: /rss/",
        "Allow: /$",
      ].join("\n"),
    );
    expect(isAllowed(rules, "/rss/allArticle.xml")).toBe(true);
    expect(isAllowed(rules, "/")).toBe(true);
    expect(isAllowed(rules, "/news/1")).toBe(false);
  });

  it("우리 이름(ripple) 그룹이 있으면 그것만 쓴다", () => {
    const rules = parseRobots(
      [
        "User-agent: *",
        "Allow: /",
        "",
        "User-agent: Ripple",
        "Disallow: /private",
      ].join("\n"),
    );
    expect(isAllowed(rules, "/private/x")).toBe(false);
    expect(isAllowed(rules, "/rss")).toBe(true);
  });

  it("연달아 쓴 User-agent는 한 그룹, * 와 $ 패턴, 빈 Disallow는 허용", () => {
    const rules = parseRobots(
      [
        "User-agent: a",
        "User-agent: *",
        "Disallow: /*?*cp=",
        "Disallow: /search$",
        "Disallow:",
      ].join("\n"),
    );
    expect(isAllowed(rules, "/view/1?cp=x")).toBe(false);
    expect(isAllowed(rules, "/search")).toBe(false);
    expect(isAllowed(rules, "/search/more")).toBe(true);
    expect(
      isAllowed(parseRobots("User-agent: *\nDisallow:"), "/anything"),
    ).toBe(true);
  });

  it("길이가 같으면 허용이 이긴다, 규칙이 없으면 허용", () => {
    expect(
      isAllowed(
        [
          { allow: false, pattern: "/a" },
          { allow: true, pattern: "/a" },
        ],
        "/a",
      ),
    ).toBe(true);
    expect(isAllowed([], "/x")).toBe(true);
  });
});

function fakeHttp(
  robots: Record<string, { status: number; text?: string } | Error>,
) {
  const calls: string[] = [];
  const http: HttpClient = {
    async get(url) {
      calls.push(url);
      const r = robots[new URL(url).origin];
      if (!r) return { status: 404, text: "", contentType: "" };
      if (r instanceof Error) throw r;
      return {
        status: r.status,
        text: r.text ?? "",
        contentType: "text/plain",
      };
    },
  };
  return { http, calls };
}

describe("createRobotsChecker", () => {
  it("사이트마다 한 번 받아 하루 동안 기억한다", async () => {
    let now = 0;
    const { http, calls } = fakeHttp({
      "https://a.example": {
        status: 200,
        text: "User-agent: *\nDisallow: /private",
      },
    });
    const robots = createRobotsChecker(http, { now: () => now });
    expect(await robots.allowed("https://a.example/rss.xml")).toBe(true);
    expect(await robots.allowed("https://a.example/private/x")).toBe(false);
    expect(calls).toEqual(["https://a.example/robots.txt"]);
    now = 25 * 60 * 60 * 1000;
    await robots.allowed("https://a.example/rss.xml");
    expect(calls).toHaveLength(2);
  });

  it("robots.txt가 없으면(4xx) 허용, 서버 오류·연결 실패면 이번엔 막는다(기억하지 않음)", async () => {
    const { http, calls } = fakeHttp({
      "https://down.example": { status: 503 },
      "https://err.example": new Error("network"),
    });
    const robots = createRobotsChecker(http);
    expect(await robots.allowed("https://none.example/feed")).toBe(true);
    expect(await robots.allowed("https://down.example/feed")).toBe(false);
    expect(await robots.allowed("https://err.example/feed")).toBe(false);
    await robots.allowed("https://down.example/feed");
    expect(calls.filter((c) => c.includes("down.example"))).toHaveLength(2);
  });
});
