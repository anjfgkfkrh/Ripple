import { describe, expect, it } from "vitest";
import {
  COLLECTOR_USER_AGENT,
  createHttpClient,
  decode,
  HttpError,
} from "./http";

function fakeFetch(
  respond: (url: string, init?: RequestInit) => Response | Promise<Response>,
) {
  const calls: { url: string; at: number; headers: Record<string, string> }[] =
    [];
  const impl = (async (input: string | URL | Request, init?: RequestInit) => {
    const url = String(input);
    calls.push({
      url,
      at: Date.now(),
      headers: (init?.headers ?? {}) as Record<string, string>,
    });
    return respond(url, init);
  }) as typeof fetch;
  return { impl, calls };
}

describe("createHttpClient", () => {
  it("같은 사이트에는 요청 시작 간격을 두고, 다른 사이트는 기다리지 않는다", async () => {
    const { impl, calls } = fakeFetch(() => new Response("ok"));
    const http = createHttpClient({ fetchImpl: impl, minIntervalMs: 80 });
    const start = Date.now();
    await Promise.all([
      http.get("https://a.example/1"),
      http.get("https://a.example/2"),
      http.get("https://b.example/1"),
    ]);
    const a = calls
      .filter((c) => c.url.includes("a.example"))
      .map((c) => c.at - start);
    const b = calls
      .filter((c) => c.url.includes("b.example"))
      .map((c) => c.at - start);
    expect(a[1] - a[0]).toBeGreaterThanOrEqual(75);
    expect(b[0]).toBeLessThan(40);
  });

  it("개인용 RSS 리더임을 밝히는 User-Agent를 보내고, 헤더를 더할 수 있다", async () => {
    const { impl, calls } = fakeFetch(() => new Response("{}"));
    const http = createHttpClient({ fetchImpl: impl, minIntervalMs: 0 });
    await http.get("https://api.example/x", { "X-Token": "t" });
    expect(calls[0].headers["User-Agent"]).toBe(COLLECTOR_USER_AGENT);
    expect(calls[0].headers["X-Token"]).toBe("t");
    expect(COLLECTOR_USER_AGENT).toContain("Ripple");
  });

  it("상태 코드와 Content-Type을 돌려준다", async () => {
    const { impl } = fakeFetch(
      () =>
        new Response("missing", {
          status: 404,
          headers: { "content-type": "text/plain" },
        }),
    );
    const http = createHttpClient({ fetchImpl: impl, minIntervalMs: 0 });
    expect(await http.get("https://e.com/x")).toEqual({
      status: 404,
      text: "missing",
      contentType: "text/plain",
    });
  });

  it("너무 큰 응답·연결 실패·시간 초과는 HttpError", async () => {
    const big = createHttpClient({
      fetchImpl: fakeFetch(() => new Response("x".repeat(100))).impl,
      minIntervalMs: 0,
      maxBytes: 10,
    });
    await expect(big.get("https://e.com/big")).rejects.toMatchObject({
      kind: "too_large",
    });

    const down = createHttpClient({
      fetchImpl: fakeFetch(() => Promise.reject(new TypeError("fetch failed")))
        .impl,
      minIntervalMs: 0,
    });
    await expect(down.get("https://e.com/x")).rejects.toBeInstanceOf(HttpError);

    const slow = createHttpClient({
      fetchImpl: fakeFetch(
        (_url, init) =>
          new Promise<Response>((_resolve, reject) => {
            init?.signal?.addEventListener("abort", () =>
              reject(init.signal?.reason),
            );
          }),
      ).impl,
      minIntervalMs: 0,
      timeoutMs: 30,
    });
    await expect(slow.get("https://e.com/slow")).rejects.toMatchObject({
      kind: "timeout",
    });
  });
});

describe("decode", () => {
  it("Content-Type이나 XML 선언의 인코딩을 따른다(EUC-KR 피드)", () => {
    // "삼성" (EUC-KR)
    const eucKr = new Uint8Array([0xbb, 0xef, 0xbc, 0xba]);
    expect(decode(eucKr, "text/xml; charset=EUC-KR")).toBe("삼성");
    const declared = new Uint8Array([
      ...new TextEncoder().encode('<?xml version="1.0" encoding="euc-kr"?>'),
      ...eucKr,
    ]);
    expect(decode(declared, "text/xml")).toContain("삼성");
    expect(decode(eucKr, "text/xml; charset=cp949")).toBe("삼성");
  });

  it("모르는 인코딩이면 UTF-8로 읽는다", () => {
    expect(
      decode(new TextEncoder().encode("가"), "text/xml; charset=none-such"),
    ).toBe("가");
  });
});
