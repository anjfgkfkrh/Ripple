import { describe, expect, it } from "vitest";
import { fetchTunnelStatus } from "./tunnel";

const respond = (body: unknown, status = 200) =>
  (async () =>
    new Response(JSON.stringify(body), {
      status,
      headers: { "content-type": "application/json" },
    })) as unknown as typeof fetch;

describe("fetchTunnelStatus (9단계)", () => {
  it("터널 주소를 정하지 않았으면 off(로컬 실행)", async () => {
    expect(await fetchTunnelStatus(undefined)).toEqual({ state: "off" });
    expect(await fetchTunnelStatus("  ")).toEqual({ state: "off" });
  });

  it("/quicktunnel의 hostname으로 https 주소를 만든다. 비어 있으면 받는 중", async () => {
    const asked: string[] = [];
    const fetchImpl = (async (url: string | URL | Request) => {
      asked.push(String(url));
      return new Response(
        JSON.stringify({ hostname: "word-word-word.trycloudflare.com" }),
      );
    }) as typeof fetch;
    expect(
      await fetchTunnelStatus("http://tunnel:20241", { fetchImpl }),
    ).toEqual({
      state: "ready",
      url: "https://word-word-word.trycloudflare.com",
    });
    expect(asked).toEqual(["http://tunnel:20241/quicktunnel"]);
    expect(
      await fetchTunnelStatus("http://tunnel:20241", {
        fetchImpl: respond({ hostname: "" }),
      }),
    ).toEqual({ state: "pending" });
  });

  it("닿지 못하거나 응답이 이상하면 error와 이유", async () => {
    expect(
      await fetchTunnelStatus("http://tunnel:20241", {
        fetchImpl: respond({}, 503),
      }),
    ).toEqual({ state: "error", reason: "HTTP 503" });
    expect(
      await fetchTunnelStatus("http://tunnel:20241", {
        fetchImpl: respond({ hostname: "<script>" }),
      }),
    ).toEqual({ state: "error", reason: "주소 형식이 다릅니다." });
    const offline = (async () => {
      throw new TypeError("fetch failed", {
        cause: new Error("getaddrinfo ENOTFOUND tunnel"),
      });
    }) as typeof fetch;
    expect(
      await fetchTunnelStatus("http://tunnel:20241", { fetchImpl: offline }),
    ).toEqual({
      state: "error",
      reason: "fetch failed (getaddrinfo ENOTFOUND tunnel)",
    });
  });
});
