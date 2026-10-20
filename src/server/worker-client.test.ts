import type { Server } from "node:http";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { afterEach, describe, expect, it } from "vitest";
import { createWorkerServer } from "@/worker/server";
import { createTestDbFile } from "../../tests/db";
import {
  fakeCollector,
  fakeKeywordQueue,
  fakeStockLlm,
  seedListings,
} from "../../tests/fixtures";
import { openDb, type Db } from "./db/client";
import { createWorkerApi } from "./worker-client";

const TOKEN = "client-test-token-0123456789abcdef";
const servers: Server[] = [];
const clients: Db[] = [];
const cleanups: (() => void)[] = [];

async function listen(server: Server): Promise<string> {
  servers.push(server);
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  return `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
}

async function startWorker(): Promise<string> {
  const file = createTestDbFile();
  cleanups.push(file.cleanup);
  const db = openDb(file.path, "readwrite");
  clients.push(db);
  seedListings(db);
  return listen(
    createWorkerServer({
      db,
      apiToken: TOKEN,
      llm: fakeStockLlm(),
      keywordQueue: fakeKeywordQueue(),
      collector: fakeCollector(),
    }),
  );
}

afterEach(async () => {
  await Promise.all(
    servers
      .splice(0)
      .map((server) => new Promise((resolve) => server.close(resolve))),
  );
  clients.splice(0).forEach((db) => db.$client.close());
  cleanups.splice(0).forEach((cleanup) => cleanup());
});

describe("web → worker 내부 API (SPEC D-36)", () => {
  it("토큰을 붙여 찾기·등록·키워드 다시 만들기·삭제를 부른다", async () => {
    const api = createWorkerApi({
      workerUrl: await startWorker(),
      token: TOKEN,
    });

    const match = await api.matchStocks("NVDA");
    expect(match.ok && match.data.candidates[0]).toMatchObject({
      code: "NVDA",
      nameKo: "엔비디아",
    });

    const registered = await api.registerStock({
      market: "US",
      code: "NVDA",
      userInput: "NVDA",
      nameKo: "엔비디아",
    });
    expect(registered.ok && registered.data.status).toBe("created");
    if (!registered.ok) return;
    const { id } = registered.data.stock;

    expect(await api.regenerateKeywords(id)).toEqual({
      ok: true,
      status: 202,
      data: { status: "queued" },
    });
    expect((await api.deleteStock(id)).ok).toBe(true);
    expect(await api.regenerateKeywords(id)).toMatchObject({
      ok: false,
      status: 404,
      error: "not_found",
    });
  });

  it("worker가 거절하면 오류 코드와 본문을 그대로 돌려준다", async () => {
    const api = createWorkerApi({
      workerUrl: await startWorker(),
      token: "wrong",
    });
    expect(await api.matchStocks("삼전")).toEqual({
      ok: false,
      status: 401,
      error: "unauthorized",
      body: { error: "unauthorized" },
    });
  });

  it("worker가 꺼져 있으면 worker_unreachable", async () => {
    const closed = createServer();
    await new Promise<void>((resolve) =>
      closed.listen(0, "127.0.0.1", resolve),
    );
    const { port } = closed.address() as AddressInfo;
    await new Promise((resolve) => closed.close(resolve));

    const api = createWorkerApi({
      workerUrl: `http://127.0.0.1:${port}`,
      token: TOKEN,
    });
    expect(await api.deleteStock(1)).toMatchObject({
      ok: false,
      status: 0,
      error: "worker_unreachable",
    });
  });
});
