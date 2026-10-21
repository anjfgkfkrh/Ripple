import type { Server } from "node:http";
import type { AddressInfo } from "node:net";
import { afterEach, describe, expect, it } from "vitest";
import type { LlmClient } from "@/server/ai/llm/types";
import { LlmError } from "@/server/ai/llm/types";
import { ModelListError } from "@/server/ai/llm/models";
import { recordUsage } from "@/server/budget/usage";
import { openDb, type Db } from "@/server/db/client";
import { replaceKeywords } from "@/server/keywords/generate";
import { readSettingsForm } from "@/server/settings-form";
import { listActiveStocks, registerStock } from "@/server/stocks/repository";
import { createTestDbFile } from "../../tests/db";
import {
  fakeCollector,
  fakeKeywordQueue,
  fakeStockLlm,
  seedListings,
} from "../../tests/fixtures";
import { createWorkerServer } from "./server";

const TOKEN = "worker-test-token-0123456789abcdef";
const servers: Server[] = [];
const clients: Db[] = [];
const cleanups: (() => void)[] = [];

async function start(
  options: {
    llm?: LlmClient;
    seeded?: boolean;
    checkModel?: (id: string) => Promise<boolean>;
  } = {},
) {
  const file = createTestDbFile();
  cleanups.push(file.cleanup);
  const db = openDb(file.path, "readwrite");
  clients.push(db);
  if (options.seeded ?? true) seedListings(db);
  const keywordQueue = fakeKeywordQueue();
  const collector = fakeCollector();
  const server = createWorkerServer({
    db,
    apiToken: TOKEN,
    llm: options.llm ?? fakeStockLlm(),
    keywordQueue,
    collector,
    checkModel: options.checkModel ?? (async () => true),
  });
  servers.push(server);
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address() as AddressInfo;
  return { baseUrl: `http://127.0.0.1:${port}`, db, keywordQueue, collector };
}

function call(
  baseUrl: string,
  path: string,
  init: { method?: string; body?: unknown; token?: string | null } = {},
) {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (init.token !== null)
    headers.Authorization = `Bearer ${init.token ?? TOKEN}`;
  return fetch(`${baseUrl}${path}`, {
    method: init.method ?? "POST",
    headers,
    body:
      init.body === undefined
        ? undefined
        : typeof init.body === "string"
          ? init.body
          : JSON.stringify(init.body),
  });
}

afterEach(async () => {
  await Promise.all(
    servers
      .splice(0)
      .map((server) => new Promise((resolve) => server.close(resolve))),
  );
  clients.splice(0).forEach((db) => {
    if (db.$client.open) db.$client.close();
  });
  cleanups.splice(0).forEach((cleanup) => cleanup());
});

describe("worker 내부 API: 공통", () => {
  it("GET /health는 토큰 없이 DB 상태와 공식 목록 건수를 준다", async () => {
    const { baseUrl } = await start();
    const response = await fetch(`${baseUrl}/health`);
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({
      status: "ok",
      db: "ok",
      listings: { KR: 7, US: 9 },
    });
  });

  it("GET /health: DB 연결이 끊기면 503", async () => {
    const { baseUrl, db } = await start();
    db.$client.close();
    expect((await fetch(`${baseUrl}/health`)).status).toBe(503);
  });

  it("다른 경로는 토큰이 없거나 틀리면 401", async () => {
    const { baseUrl } = await start();
    const body = { query: "삼전" };
    expect(
      (await call(baseUrl, "/stocks/match", { body, token: null })).status,
    ).toBe(401);
    expect(
      (await call(baseUrl, "/stocks/match", { body, token: `${TOKEN}x` }))
        .status,
    ).toBe(401);
  });

  it("없는 경로는 404, 잘못된 본문은 400, 너무 큰 본문은 413", async () => {
    const { baseUrl } = await start();
    expect((await call(baseUrl, "/nope", { body: {} })).status).toBe(404);
    expect(
      (await call(baseUrl, "/stocks/match", { body: "{not json" })).status,
    ).toBe(400);
    expect(
      (await call(baseUrl, "/stocks/match", { body: { query: "" } })).status,
    ).toBe(400);
    expect(
      (
        await call(baseUrl, "/stocks/match", {
          body: { query: "x".repeat(51) },
        })
      ).status,
    ).toBe(400);
    expect(
      (
        await call(baseUrl, "/stocks/match", {
          body: { query: "x".repeat(20_000) },
        })
      ).status,
    ).toBe(413);
  });
});

describe("worker 내부 API: 종목 찾기", () => {
  it("POST /stocks/match → 공식 목록과 대조한 후보", async () => {
    const { baseUrl } = await start();
    const response = await call(baseUrl, "/stocks/match", {
      body: { query: "삼전" },
    });
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({
      candidates: [
        {
          market: "KR",
          code: "005930",
          name: "삼성전자",
          nameKo: "삼성전자",
          exchange: "KOSPI",
          isEtf: false,
          alreadyRegistered: false,
        },
      ],
    });
  });

  it("예산을 다 썼으면 402와 사용액·한도", async () => {
    const { baseUrl, db } = await start();
    recordUsage(db, {
      provider: "openrouter",
      model: "m",
      purpose: "t",
      usage: { inputTokens: 1, outputTokens: 1, costUsd: 5 },
    });
    const response = await call(baseUrl, "/stocks/match", {
      body: { query: "삼전" },
    });
    expect(response.status).toBe(402);
    expect(await response.json()).toEqual({
      error: "budget_exceeded",
      spentUsd: 5,
      limitUsd: 5,
    });
  });

  it("공식 목록이 아직 없으면 503 listings_unavailable", async () => {
    const { baseUrl } = await start({ seeded: false });
    const response = await call(baseUrl, "/stocks/match", {
      body: { query: "삼전" },
    });
    expect(response.status).toBe(503);
    expect(await response.json()).toEqual({ error: "listings_unavailable" });
  });

  it.each([
    ["no_api_key", 503],
    ["payment_required", 402],
    ["rate_limited", 429],
    ["timeout", 504],
    ["invalid_output", 502],
  ] as const)("LLM 오류 %s → %i", async (kind, status) => {
    const llm: LlmClient = {
      provider: "openrouter",
      generateObject: () => Promise.reject(new LlmError("x", kind)),
    };
    const { baseUrl } = await start({ llm });
    const response = await call(baseUrl, "/stocks/match", {
      body: { query: "삼전" },
    });
    expect(response.status).toBe(status);
    expect(await response.json()).toEqual({ error: `llm_${kind}` });
  });
});

describe("worker 내부 API: 등록·삭제", () => {
  it("POST /stocks: 201 등록 → 409 중복 → DELETE 200 → 다시 등록 200 복원", async () => {
    const { baseUrl, db, keywordQueue } = await start();
    const input = {
      market: "US",
      code: "NVDA",
      userInput: "엔비디아",
      nameKo: "엔비디아",
    };

    const created = await call(baseUrl, "/stocks", { body: input });
    expect(created.status).toBe(201);
    const { stock } = (await created.json()) as { stock: { id: number } };

    expect((await call(baseUrl, "/stocks", { body: input })).status).toBe(409);

    const deleted = await call(baseUrl, `/stocks/${stock.id}`, {
      method: "DELETE",
    });
    expect(deleted.status).toBe(200);
    expect(listActiveStocks(db)).toEqual([]);
    expect(
      (await call(baseUrl, `/stocks/${stock.id}`, { method: "DELETE" })).status,
    ).toBe(404);

    const restored = await call(baseUrl, "/stocks", { body: input });
    expect(restored.status).toBe(200);
    expect(await restored.json()).toMatchObject({ status: "restored" });

    // 등록과 복원 때만 키워드를 새로 만든다(중복 등록은 아님, D-37).
    expect(keywordQueue.enqueued).toEqual([stock.id, stock.id]);
  });

  it("공식 목록에 없으면 404 not_listed", async () => {
    const { baseUrl } = await start();
    const response = await call(baseUrl, "/stocks", {
      body: { market: "KR", code: "123456", userInput: "x" },
    });
    expect(response.status).toBe(404);
    expect(await response.json()).toEqual({ error: "not_listed" });
  });
});

describe("worker 내부 API: 키워드 다시 만들기", () => {
  async function registered() {
    const started = await start();
    const response = await call(started.baseUrl, "/stocks", {
      body: { market: "KR", code: "005930", userInput: "삼전" },
    });
    const { stock } = (await response.json()) as { stock: { id: number } };
    started.keywordQueue.enqueued.length = 0;
    return { ...started, id: stock.id };
  }

  it("POST /stocks/:id/keywords → 202, 대기열에 넣음", async () => {
    const { baseUrl, keywordQueue, id } = await registered();
    const response = await call(baseUrl, `/stocks/${id}/keywords`);
    expect(response.status).toBe(202);
    expect(await response.json()).toEqual({ status: "queued" });
    expect(keywordQueue.enqueued).toEqual([id]);
  });

  it("없거나 삭제한 종목은 404, 토큰이 없으면 401", async () => {
    const { baseUrl, keywordQueue, id } = await registered();
    expect((await call(baseUrl, `/stocks/${id + 1}/keywords`)).status).toBe(
      404,
    );
    await call(baseUrl, `/stocks/${id}`, { method: "DELETE" });
    expect((await call(baseUrl, `/stocks/${id}/keywords`)).status).toBe(404);
    expect(
      (await call(baseUrl, `/stocks/${id}/keywords`, { token: null })).status,
    ).toBe(401);
    expect(keywordQueue.enqueued).toEqual([]);
  });
});

describe("worker 내부 API: 기사 수집", () => {
  it("POST /collect → 202와 회차 id, 도는 중에 다시 부르면 409", async () => {
    const { baseUrl, collector } = await start();
    const first = await call(baseUrl, "/collect");
    expect(first.status).toBe(202);
    expect(await first.json()).toEqual({ runId: 1 });
    expect(collector.started).toEqual(["manual"]);

    const again = await call(baseUrl, "/collect");
    expect(again.status).toBe(409);
    expect(await again.json()).toEqual({ error: "already_running", runId: 1 });

    collector.finish();
    expect((await call(baseUrl, "/collect")).status).toBe(202);
  });

  it("토큰이 없으면 401", async () => {
    const { baseUrl, collector } = await start();
    expect((await call(baseUrl, "/collect", { token: null })).status).toBe(401);
    expect(collector.started).toEqual([]);
  });
});

describe("worker 내부 API: 설정 (SPEC FR-8)", () => {
  const form = {
    collectIntervalMinutes: "60",
    keywordsDepth: "3",
    keywordsRegenerateEvery: "weekly",
    minRelevance: "6.5",
    maxPerRun: "150",
    monthlyBudgetUsd: "7.5",
    llmModel: "anthropic/claude-haiku-5.5",
  };

  it("PUT /settings: 검사한 값을 한 번에 저장하고 바뀐 항목을 알려 준다", async () => {
    const checked: string[] = [];
    const { baseUrl, db } = await start({
      checkModel: async (id) => {
        checked.push(id);
        return true;
      },
    });
    const response = await call(baseUrl, "/settings", {
      method: "PUT",
      body: form,
    });
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({
      ok: true,
      changed: [
        "collectIntervalMinutes",
        "keywordsDepth",
        "keywordsRegenerateEvery",
        "minRelevance",
        "maxPerRun",
        "monthlyBudgetUsd",
      ],
    });
    expect(readSettingsForm(db)).toEqual({
      collectIntervalMinutes: 60,
      keywordsDepth: 3,
      keywordsRegenerateEvery: "weekly",
      minRelevance: 6.5,
      maxPerRun: 150,
      monthlyBudgetUsd: 7.5,
      llmModel: "anthropic/claude-haiku-5.5",
    });
    // 모델을 바꾸지 않았으면 OpenRouter에 묻지 않는다.
    expect(checked).toEqual([]);
  });

  it("값이 틀리면 400과 항목별 이유, 아무것도 저장하지 않는다", async () => {
    const { baseUrl, db } = await start();
    const response = await call(baseUrl, "/settings", {
      method: "PUT",
      body: {
        ...form,
        collectIntervalMinutes: "45",
        minRelevance: "11",
        llmModel: "모델",
      },
    });
    expect(response.status).toBe(400);
    expect(await response.json()).toEqual({
      error: "invalid_settings",
      fields: {
        collectIntervalMinutes: "15·30·60·180분 중에서 고르세요.",
        minRelevance: "0~10 사이로 넣으세요.",
        llmModel: "모델 ID 형식이 아닙니다(예: anthropic/claude-haiku-5.5).",
      },
    });
    expect(readSettingsForm(db).collectIntervalMinutes).toBe(30);
  });

  it("모델을 바꾸면 OpenRouter 목록으로 확인하고, 없거나 확인하지 못하면 저장하지 않는다", async () => {
    const unknown = await start({ checkModel: async () => false });
    const body = { ...form, llmModel: "openai/gpt-none" };
    const notFound = await call(unknown.baseUrl, "/settings", {
      method: "PUT",
      body,
    });
    expect(notFound.status).toBe(422);
    expect(await notFound.json()).toEqual({ error: "unknown_model" });
    expect(readSettingsForm(unknown.db).collectIntervalMinutes).toBe(30);

    const offline = await start({
      checkModel: async () => {
        throw new ModelListError("연결 실패");
      },
    });
    const failed = await call(offline.baseUrl, "/settings", {
      method: "PUT",
      body,
    });
    expect(failed.status).toBe(502);
    expect(await failed.json()).toEqual({ error: "model_check_failed" });

    const known = await start({ checkModel: async () => true });
    const saved = await call(known.baseUrl, "/settings", {
      method: "PUT",
      body,
    });
    expect(saved.status).toBe(200);
    expect(readSettingsForm(known.db).llmModel).toBe("openai/gpt-none");
  });

  it("POST /keywords/rebuild: 저장한 깊이와 다른 깊이로 만든 종목만 다시 만든다", async () => {
    const { baseUrl, db, keywordQueue } = await start();
    const deep = registerStock(db, {
      market: "KR",
      code: "005930",
      userInput: "삼전",
    });
    const fresh = registerStock(db, {
      market: "KR",
      code: "000660",
      userInput: "하닉",
    });
    if (deep.status === "not_listed" || fresh.status === "not_listed")
      throw new Error("표본에 없음");
    const node = (key: number, level: number, parentKey: number | null) => ({
      key,
      parentKey,
      level,
      textKo: `키워드 ${key}`,
      textEn: null,
      reason: "r",
      path: `경로 ${key}`,
    });
    // 삼성전자는 깊이 1, SK하이닉스는 깊이 2(기본)로 만들어져 있다.
    replaceKeywords(db, deep.stock.id, [node(1, 1, null)], new Date());
    replaceKeywords(
      db,
      fresh.stock.id,
      [node(1, 1, null), node(2, 2, 1)],
      new Date(),
    );

    const response = await call(baseUrl, "/keywords/rebuild");
    expect(response.status).toBe(202);
    expect(await response.json()).toEqual({ count: 1 });
    expect(keywordQueue.enqueued).toEqual([deep.stock.id]);
  });
});
