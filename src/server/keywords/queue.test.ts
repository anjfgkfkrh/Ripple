import { eq } from "drizzle-orm";
import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import {
  fakeLlm,
  sampleKeywordTree,
  seedListings,
} from "../../../tests/fixtures";
import { LlmError, type LlmClient } from "../ai/llm/types";
import { openDb, type Db } from "../db/client";
import { stocks } from "../db/schema";
import { registerStock } from "../stocks/repository";
import { createKeywordQueue } from "./queue";

const opened: Db[] = [];
const cleanups: (() => void)[] = [];
function testDb(): Db {
  const file = createTestDbFile();
  cleanups.push(file.cleanup);
  const db = openDb(file.path, "readwrite");
  opened.push(db);
  seedListings(db);
  return db;
}
afterEach(() => {
  opened.splice(0).forEach((db) => db.$client.close());
  cleanups.splice(0).forEach((cleanup) => cleanup());
});

/** 표본의 한국 종목 3개를 등록한다. */
function registerThree(db: Db): number[] {
  return ["005930", "000660", "000720"].map((code) => {
    const result = registerStock(db, { market: "KR", code, userInput: code });
    if (result.status === "not_listed") throw new Error(code);
    return result.stock.id;
  });
}

const status = (db: Db, id: number) =>
  db.select().from(stocks).where(eq(stocks.id, id)).get()?.keywordsStatus;

/** 부를 때마다 동시에 몇 개가 돌고 있는지 재는 가짜 LLM */
function slowLlm() {
  let active = 0;
  let maxActive = 0;
  const llm = fakeLlm(() => sampleKeywordTree());
  const wrapped: LlmClient = {
    provider: "openrouter",
    async generateObject(request) {
      active += 1;
      maxActive = Math.max(maxActive, active);
      await new Promise((resolve) => setTimeout(resolve, 10));
      try {
        return await llm.generateObject(request);
      } finally {
        active -= 1;
      }
    },
  };
  return { llm: wrapped, calls: llm.calls, maxActive: () => maxActive };
}

describe("키워드 생성 대기열 (D-37)", () => {
  it("넣으면 바로 대기 상태가 되고, 하나씩 차례로 만든다", async () => {
    const db = testDb();
    const ids = registerThree(db);
    db.update(stocks).set({ keywordsStatus: "ready" }).run();
    const slow = slowLlm();
    const queue = createKeywordQueue({ db, llm: slow.llm });

    for (const id of ids) queue.enqueue(id);
    // 첫 종목은 바로 만들기 시작하고, 나머지는 기다린다.
    expect(ids.map((id) => status(db, id))).toEqual([
      "generating",
      "pending",
      "pending",
    ]);

    await queue.idle();
    expect(ids.map((id) => status(db, id))).toEqual([
      "ready",
      "ready",
      "ready",
    ]);
    expect(slow.calls).toHaveLength(3);
    expect(slow.maxActive()).toBe(1);
  });

  it("이미 기다리거나 만드는 중인 종목을 다시 넣으면 무시한다", async () => {
    const db = testDb();
    const [first, second] = registerThree(db);
    const slow = slowLlm();
    const queue = createKeywordQueue({ db, llm: slow.llm });

    queue.enqueue(first);
    queue.enqueue(second);
    queue.enqueue(first); // 만드는 중
    queue.enqueue(second); // 기다리는 중
    await queue.idle();
    expect(slow.calls).toHaveLength(2);

    // 끝난 뒤에 넣으면 다시 만든다.
    queue.enqueue(first);
    await queue.idle();
    expect(slow.calls).toHaveLength(3);
  });

  it("한 종목이 실패해도 다음 종목은 계속 만든다", async () => {
    const db = testDb();
    const [first, second] = registerThree(db);
    let call = 0;
    const llm: LlmClient = {
      provider: "openrouter",
      async generateObject(request) {
        call += 1;
        if (call === 1) throw new LlmError("x", "upstream");
        return fakeLlm(() => sampleKeywordTree()).generateObject(request);
      },
    };
    const queue = createKeywordQueue({ db, llm });

    queue.enqueue(first);
    queue.enqueue(second);
    await queue.idle();
    expect([status(db, first), status(db, second)]).toEqual([
      "failed",
      "ready",
    ]);
  });

  it("비어 있으면 idle은 바로 끝난다", async () => {
    const db = testDb();
    await expect(
      createKeywordQueue({ db, llm: slowLlm().llm }).idle(),
    ).resolves.toBeUndefined();
  });
});
