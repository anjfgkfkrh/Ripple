import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { afterEach, describe, expect, it } from "vitest";
import { createWorkerServer } from "@/worker/server";
import { createTestDbFile } from "../../tests/db";
import {
  fakeCollector,
  fakeKeywordQueue,
  fakeStockLlm,
} from "../../tests/fixtures";
import { enableWal, openDb, type Db } from "./db/client";
import { getSystemStatus } from "./status";
import { fetchWorkerHealth } from "./worker-client";

const servers: Server[] = [];
const clients: Db[] = [];
const cleanups: (() => void)[] = [];

function open(dbPath: string, access: "readonly" | "readwrite") {
  const db = openDb(dbPath, access);
  clients.push(db);
  return db;
}

async function listen(server: Server): Promise<string> {
  servers.push(server);
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  return `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
}

/** 아무 것도 듣지 않는 주소: 서버를 열었다 닫아 포트를 비운다. */
async function unusedUrl(): Promise<string> {
  const server = createServer();
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address() as AddressInfo;
  await new Promise((resolve) => server.close(resolve));
  return `http://127.0.0.1:${port}`;
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

describe("getSystemStatus (0단계 확인 화면)", () => {
  it("web이 DB를 읽기 전용으로 읽고 worker 내부 API에 닿으면 둘 다 정상", async () => {
    const file = createTestDbFile();
    cleanups.push(file.cleanup);
    const writer = open(file.path, "readwrite");
    enableWal(writer);
    const workerUrl = await listen(
      createWorkerServer({
        db: writer,
        apiToken: "status-test-token-0123456789abcdef",
        llm: fakeStockLlm(),
        keywordQueue: fakeKeywordQueue(),
        collector: fakeCollector(),
        checkModel: async () => true,
      }),
    );
    const reader = open(file.path, "readonly");

    const status = await getSystemStatus({ getDb: () => reader, workerUrl });

    expect(status.database).toEqual({
      ok: true,
      journalMode: "wal",
      settingsCount: 0,
    });
    expect(status.worker).toEqual({ ok: true, listings: { KR: 0, US: 0 } });
    expect(status.checkedAt).toBeInstanceOf(Date);
  });

  it("DB를 열 수 없거나 worker 주소가 없으면 각각 오류로 표시한다", async () => {
    const status = await getSystemStatus({
      getDb: () => {
        throw new Error("환경변수 DATABASE_PATH이(가) 설정되지 않았습니다.");
      },
      workerUrl: undefined,
    });

    expect(status.database).toEqual({
      ok: false,
      reason: "환경변수 DATABASE_PATH이(가) 설정되지 않았습니다.",
    });
    expect(status.worker).toEqual({
      ok: false,
      reason: "환경변수 WORKER_URL이 설정되지 않았습니다.",
    });
  });
});

describe("fetchWorkerHealth", () => {
  it("worker가 꺼져 있으면 실패 이유를 돌려준다", async () => {
    const health = await fetchWorkerHealth(await unusedUrl());
    expect(health.ok).toBe(false);
    if (!health.ok) expect(health.reason).toMatch(/ECONNREFUSED|fetch failed/);
  });

  it("200이 아니거나 형식이 다르면 실패로 본다", async () => {
    const url500 = await listen(
      createServer((_, response) => {
        response.writeHead(500).end();
      }),
    );
    expect(await fetchWorkerHealth(url500)).toEqual({
      ok: false,
      reason: "HTTP 500",
    });

    const urlWrongBody = await listen(
      createServer((_, response) => {
        response
          .writeHead(200, { "content-type": "application/json" })
          .end("{}");
      }),
    );
    expect(await fetchWorkerHealth(urlWrongBody)).toEqual({
      ok: false,
      reason: "응답 형식이 올바르지 않습니다.",
    });
  });

  it("응답이 늦으면 제한 시간 뒤 실패한다", async () => {
    const slowUrl = await listen(createServer(() => {}));
    const startedAt = Date.now();
    const health = await fetchWorkerHealth(slowUrl, 200);
    expect(health.ok).toBe(false);
    expect(Date.now() - startedAt).toBeLessThan(2000);
  });
});
