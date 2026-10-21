import { describe, expect, it } from "vitest";
import { isOpenRouterModel, ModelListError } from "./models";

/** 주소별로 정한 응답을 주는 가짜 fetch */
function fakeFetch(pages: Record<string, { status?: number; body: unknown }>) {
  const asked: string[] = [];
  const fetchImpl = (async (url: string | URL | Request) => {
    asked.push(String(url));
    const page = pages[String(url)];
    if (!page) throw new TypeError("fetch failed");
    return new Response(JSON.stringify(page.body), {
      status: page.status ?? 200,
      headers: { "content-type": "application/json" },
    });
  }) as typeof fetch;
  return { fetchImpl, asked };
}

const BASE = "http://openrouter.test/api/v1";

describe("isOpenRouterModel (2026-10-21 결정)", () => {
  it("공개 모델 목록에 있으면 true, 없으면 false. 다음 쪽이 있으면 따라간다", async () => {
    const { fetchImpl, asked } = fakeFetch({
      [`${BASE}/models`]: {
        body: {
          data: [{ id: "anthropic/claude-haiku-5.5" }],
          links: { next: `${BASE}/models?page=2` },
        },
      },
      [`${BASE}/models?page=2`]: {
        body: { data: [{ id: "openai/gpt-x" }], links: { next: null } },
      },
    });
    const check = (id: string) =>
      isOpenRouterModel(id, { baseUrl: BASE, fetchImpl });

    expect(await check("anthropic/claude-haiku-5.5")).toBe(true);
    expect(asked).toEqual([`${BASE}/models`]);
    expect(await check("openai/gpt-x")).toBe(true);
    expect(await check("nobody/none")).toBe(false);
  });

  it("목록을 받지 못하거나 형식이 다르면 ModelListError(저장하지 않음)", async () => {
    const failing = fakeFetch({
      [`${BASE}/models`]: { status: 503, body: {} },
    });
    await expect(
      isOpenRouterModel("a/b", { baseUrl: BASE, fetchImpl: failing.fetchImpl }),
    ).rejects.toBeInstanceOf(ModelListError);

    const offline = fakeFetch({});
    await expect(
      isOpenRouterModel("a/b", { baseUrl: BASE, fetchImpl: offline.fetchImpl }),
    ).rejects.toThrow("OpenRouter 모델 목록을 받지 못했습니다");

    const odd = fakeFetch({ [`${BASE}/models`]: { body: { models: [] } } });
    await expect(
      isOpenRouterModel("a/b", { baseUrl: BASE, fetchImpl: odd.fetchImpl }),
    ).rejects.toThrow("형식이 다릅니다");
  });
});
