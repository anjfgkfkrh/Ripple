import { describe, expect, it } from "vitest";
import { createFinnhubClient, FinnhubError } from "./finnhub";
import type { HttpClient } from "./http";

function fakeHttp(respond: (url: string) => { status: number; text: string }) {
  const calls: { url: string; headers?: Record<string, string> }[] = [];
  const http: HttpClient = {
    async get(url, headers) {
      calls.push({ url, headers });
      return { ...respond(url), contentType: "application/json" };
    },
  };
  return { http, calls };
}

const NEWS = {
  id: 1,
  headline: "Nvidia rises",
  summary: "AI demand",
  url: "https://finnhub.io/api/news?id=abc",
  source: "Yahoo",
  datetime: 1_792_454_400,
  related: "NVDA",
  category: "company",
};

describe("createFinnhubClient", () => {
  it("종목 뉴스: 기간을 날짜(UTC)로 보내고 키는 헤더로 보낸다", async () => {
    const { http, calls } = fakeHttp(() => ({
      status: 200,
      text: JSON.stringify([NEWS]),
    }));
    const client = createFinnhubClient({
      http,
      apiKey: "k",
      baseUrl: "https://fh.example/api/v1/",
    });
    const news = await client.companyNews(
      "BRK.B",
      new Date("2026-10-19T03:00:00Z"),
      new Date("2026-10-20T03:00:00Z"),
    );
    expect(news).toEqual([NEWS]);
    expect(calls[0].url).toBe(
      "https://fh.example/api/v1/company-news?symbol=BRK.B&from=2026-10-19&to=2026-10-20",
    );
    expect(calls[0].headers).toEqual({ "X-Finnhub-Token": "k" });
    // 키가 주소에 남지 않는다.
    expect(calls[0].url).not.toContain("token");
  });

  it("시장 뉴스는 general 분류를 받는다", async () => {
    const { http, calls } = fakeHttp(() => ({ status: 200, text: "[]" }));
    await createFinnhubClient({ http, apiKey: "k" }).generalNews();
    expect(calls[0].url).toBe(
      "https://finnhub.io/api/v1/news?category=general",
    );
  });

  it("형식이 다른 항목은 버린다", async () => {
    const { http } = fakeHttp(() => ({
      status: 200,
      text: JSON.stringify([
        NEWS,
        { headline: 1 },
        null,
        { ...NEWS, url: undefined },
      ]),
    }));
    expect(
      await createFinnhubClient({ http, apiKey: "k" }).generalNews(),
    ).toEqual([NEWS]);
  });

  it.each([
    [401, "키가 맞지 않거나"],
    [403, "키가 맞지 않거나"],
    [429, "호출 한도"],
    [500, "HTTP 500"],
  ])("HTTP %i → 이유를 알려 주는 오류", async (status, message) => {
    const { http } = fakeHttp(() => ({ status, text: "{}" }));
    const call = createFinnhubClient({ http, apiKey: "k" }).generalNews();
    await expect(call).rejects.toBeInstanceOf(FinnhubError);
    await expect(call).rejects.toThrow(message);
  });

  it("JSON이 아니거나 배열이 아니면 오류", async () => {
    const bad = fakeHttp(() => ({ status: 200, text: "<html>" }));
    await expect(
      createFinnhubClient({ http: bad.http, apiKey: "k" }).generalNews(),
    ).rejects.toThrow("읽지 못했습니다");
    const obj = fakeHttp(() => ({ status: 200, text: '{"error":"x"}' }));
    await expect(
      createFinnhubClient({ http: obj.http, apiKey: "k" }).generalNews(),
    ).rejects.toThrow("형식이 다릅니다");
  });
});
