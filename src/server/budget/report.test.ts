import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import { openDb, type Db } from "../db/client";
import { writeSetting } from "../settings";
import { budgetLevel, monthUsageReport } from "./report";
import { recordUsage } from "./usage";

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

// 한국 시간 2026-10-21 12:00
const NOW = new Date("2026-10-21T03:00:00Z");

describe("budgetLevel (D-07)", () => {
  it("80%부터 미리 알림, 100%부터 멈춤", () => {
    expect(budgetLevel(3.99, 5)).toBe("ok");
    expect(budgetLevel(4, 5)).toBe("warn");
    expect(budgetLevel(5, 5)).toBe("over");
    expect(budgetLevel(6, 5)).toBe("over");
  });
});

describe("monthUsageReport (FR-8)", () => {
  it("이번 달 기록만 제공자·용도별로 묶어 비용이 큰 것부터, 한도와 단계를 함께", () => {
    const db = testDb();
    const log = (
      provider: "openrouter" | "typesafe",
      purpose: string,
      costUsd: number,
      at = NOW,
    ) =>
      recordUsage(db, {
        provider,
        model: "m",
        purpose,
        usage: { inputTokens: 1, outputTokens: 1, costUsd },
        at,
      });
    log("typesafe", "article_evaluation", 0.01);
    log("typesafe", "article_evaluation", 0.02);
    log("openrouter", "keyword_generation", 0.005);
    log("openrouter", "stock_match", 0.001);
    // 지난달(한국 9월 30일 23:00)
    log("openrouter", "stock_match", 1, new Date("2026-09-30T14:00:00Z"));
    writeSetting(db, "budget.monthlyUsd", 0.04);

    const report = monthUsageReport(db, NOW);
    expect(report.rows).toEqual([
      {
        provider: "typesafe",
        purpose: "article_evaluation",
        label: "기사 평가",
        calls: 2,
        costUsd: expect.closeTo(0.03, 9),
      },
      {
        provider: "openrouter",
        purpose: "keyword_generation",
        label: "키워드 만들기",
        calls: 1,
        costUsd: expect.closeTo(0.005, 9),
      },
      {
        provider: "openrouter",
        purpose: "stock_match",
        label: "종목 찾기",
        calls: 1,
        costUsd: expect.closeTo(0.001, 9),
      },
    ]);
    expect(report.totalUsd).toBeCloseTo(0.036, 9);
    expect(report.limitUsd).toBe(0.04);
    expect(report.level).toBe("warn");
  });

  it("시장 이슈 정리는 따로 보이고, 화면 이름이 같은 품질 확인은 제공자마다 한 줄로 합친다(10단계)", () => {
    const db = testDb();
    const log = (
      provider: "openrouter" | "typesafe",
      purpose: string,
      costUsd: number,
    ) =>
      recordUsage(db, {
        provider,
        model: "m",
        purpose,
        usage: { inputTokens: 1, outputTokens: 1, costUsd },
        at: NOW,
      });
    log("openrouter", "market_issues", 0.007);
    log("openrouter", "keyword_generation_check", 0.003);
    log("openrouter", "market_issues_check", 0.002);
    log("openrouter", "model_compare", 0.001);
    log("typesafe", "article_evaluation_check", 0.004);

    expect(
      monthUsageReport(db, NOW).rows.map((row) => [
        row.provider,
        row.label,
        row.calls,
        Number(row.costUsd.toFixed(6)),
      ]),
    ).toEqual([
      ["openrouter", "시장 이슈 정리", 1, 0.007],
      ["openrouter", "품질 확인(개발 중 직접 호출)", 3, 0.006],
      ["typesafe", "품질 확인(개발 중 직접 호출)", 1, 0.004],
    ]);
  });
});
