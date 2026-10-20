import { afterEach, describe, expect, it } from "vitest";
import * as z from "zod";
import { createTestDbFile } from "../../../tests/db";
import { fakeLlm } from "../../../tests/fixtures";
import { LlmError, type StructuredRequest } from "../ai/llm/types";
import { openDb, type Db } from "../db/client";
import { usageLogs } from "../db/schema";
import { writeSetting } from "../settings";
import {
  BudgetExceededError,
  generateWithBudget,
  monthSpendUsd,
  recordUsage,
} from "./usage";

const opened: Db[] = [];
const cleanups: (() => void)[] = [];
function testDb(): Db {
  const file = createTestDbFile();
  cleanups.push(file.cleanup);
  const db = openDb(file.path, "readwrite");
  opened.push(db);
  return db;
}
afterEach(() => {
  opened.splice(0).forEach((db) => db.$client.close());
  cleanups.splice(0).forEach((cleanup) => cleanup());
});

const request: StructuredRequest<{ ok: boolean }> = {
  model: "test/model",
  messages: [{ role: "user", content: "hi" }],
  schema: z.object({ ok: z.boolean() }),
  schemaName: "ok",
  maxTokens: 100,
};

const usage = (costUsd: number) => ({
  inputTokens: 10,
  outputTokens: 5,
  costUsd,
});
// 한국 시간 2026-10-20 12:00
const NOW = new Date("2026-10-20T03:00:00Z");

describe("monthSpendUsd", () => {
  it("이번 달(한국 시간) 기록만 더한다", () => {
    const db = testDb();
    const log = (at: string, cost: number) =>
      recordUsage(db, {
        provider: "openrouter",
        model: "m",
        purpose: "test",
        usage: usage(cost),
        at: new Date(at),
      });
    log("2026-09-30T14:59:59Z", 1); // 한국 9월 30일 23:59 → 지난달
    log("2026-09-30T15:00:00Z", 0.25); // 한국 10월 1일 00:00 → 이번 달
    log("2026-10-19T10:00:00Z", 0.5);

    expect(monthSpendUsd(db, NOW)).toBeCloseTo(0.75);
  });
});

describe("generateWithBudget (CLAUDE.md: 한도 검사 → 호출 → 기록)", () => {
  it("한도 안이면 호출하고 사용량을 기록한다", async () => {
    const db = testDb();
    const llm = fakeLlm(() => ({ ok: true }), usage(0.001));

    const result = await generateWithBudget(db, llm, request, {
      purpose: "stock_match",
      now: () => NOW,
    });

    expect(result.value).toEqual({ ok: true });
    const logs = db.select().from(usageLogs).all();
    expect(logs).toHaveLength(1);
    expect(logs[0]).toMatchObject({
      provider: "openrouter",
      model: "test/model",
      purpose: "stock_match",
      inputTokens: 10,
      outputTokens: 5,
      costUsd: 0.001,
    });
  });

  it("이번 달 사용액이 한도(기본 $5)에 닿으면 호출하지 않고 막는다", async () => {
    const db = testDb();
    recordUsage(db, {
      provider: "openrouter",
      model: "m",
      purpose: "test",
      usage: usage(5),
      at: NOW,
    });
    const llm = fakeLlm(() => ({ ok: true }));

    const error = await generateWithBudget(db, llm, request, {
      purpose: "stock_match",
      now: () => NOW,
    }).catch((e: unknown) => e);

    expect(error).toBeInstanceOf(BudgetExceededError);
    expect(error).toMatchObject({ spentUsd: 5, limitUsd: 5 });
    expect(llm.calls).toHaveLength(0);
  });

  it("한도는 설정 값을 따른다", async () => {
    const db = testDb();
    writeSetting(db, "budget.monthlyUsd", 0.5);
    recordUsage(db, {
      provider: "openrouter",
      model: "m",
      purpose: "test",
      usage: usage(0.6),
      at: NOW,
    });
    await expect(
      generateWithBudget(
        db,
        fakeLlm(() => ({ ok: true })),
        request,
        {
          purpose: "t",
          now: () => NOW,
        },
      ),
    ).rejects.toBeInstanceOf(BudgetExceededError);
  });

  it("응답 형식이 틀려 실패해도 청구된 비용은 기록한다", async () => {
    const db = testDb();
    const llm = fakeLlm(() => ({ wrong: 1 }), usage(0.002));

    await expect(
      generateWithBudget(db, llm, request, { purpose: "t", now: () => NOW }),
    ).rejects.toBeInstanceOf(LlmError);
    expect(db.select().from(usageLogs).all()[0]?.costUsd).toBe(0.002);
  });
});
