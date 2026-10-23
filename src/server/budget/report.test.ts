import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import { eq } from "drizzle-orm";
import { openDb, type Db } from "../db/client";
import { stocks, topics } from "../db/schema";
import { writeSetting } from "../settings";
import {
  addUserTopic,
  deleteUserTopic,
  ensureDefaultMarket,
  setMarketEnabled,
  syncTopicLenses,
} from "../topics/store";
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
        share: expect.closeTo(0.03 / 0.036, 9),
      },
      {
        provider: "openrouter",
        purpose: "keyword_generation",
        label: "키워드 만들기",
        calls: 1,
        costUsd: expect.closeTo(0.005, 9),
        share: expect.closeTo(0.005 / 0.036, 9),
      },
      {
        provider: "openrouter",
        purpose: "stock_match",
        label: "종목 찾기",
        calls: 1,
        costUsd: expect.closeTo(0.001, 9),
        share: expect.closeTo(0.001 / 0.036, 9),
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

describe("monthUsageReport: 항목별 비용 (11단계)", () => {
  it("종목·시장·주제·공용으로 묶고 비율을 낸다. 같은 주제의 증시별 항목과 자동 토픽은 한 줄로", () => {
    const db = testDb();
    const at = NOW;
    const log = (
      purpose: string,
      costUsd: number,
      stockId: number | null,
      provider: "openrouter" | "typesafe" = "typesafe",
    ) =>
      recordUsage(db, {
        provider,
        model: "m",
        purpose,
        usage: { inputTokens: 1, outputTokens: 1, costUsd },
        at,
        stockId,
      });
    const stock = db
      .insert(stocks)
      .values({
        market: "KR",
        code: "005930",
        name: "삼성전자",
        nameKo: "삼성전자",
        exchange: "KOSPI",
        userInput: "삼전",
        createdAt: at,
      })
      .returning({ id: stocks.id })
      .get().id;
    const market = ensureDefaultMarket(db, at)!;
    setMarketEnabled(db, "US", true, at);
    const user = addUserTopic(db, "부동산 시장", at);
    if (!user.ok) throw new Error("주제");
    const auto = (name: string) =>
      syncTopicLenses(
        db,
        db
          .insert(topics)
          .values({ source: "auto", name, terms: [name], createdAt: at })
          .returning()
          .get(),
        at,
      );
    const [autoA] = auto("중동 긴장");
    const [autoB] = auto("기준금리");

    log("article_evaluation", 0.006, stock);
    log("keyword_rating", 0.001, stock);
    log("keyword_generation", 0.002, market, "openrouter");
    // 사용자 주제: 한국·미국 기준 항목 두 줄을 한 주제로
    log("article_evaluation", 0.001, user.lensIds[0]);
    log("article_evaluation", 0.001, user.lensIds[1]);
    log("article_evaluation", 0.0005, autoA);
    log("article_evaluation", 0.0005, autoB);
    log("topic_refresh", 0.005, null, "openrouter");
    // 11단계 전 기록(항목을 남기지 않음)
    log("article_evaluation", 0.003, null);

    const report = monthUsageReport(db, NOW);
    const total = 0.02;
    expect(report.totalUsd).toBeCloseTo(total, 9);
    expect(
      report.groups.map((g) => [
        g.label,
        g.calls,
        Number(g.costUsd.toFixed(6)),
      ]),
    ).toEqual([
      ["종목", 2, 0.007],
      ["시장", 1, 0.002],
      ["주제·토픽", 4, 0.003],
      ["공용", 1, 0.005],
      ["항목 기록 없음(11단계 전)", 1, 0.003],
    ]);
    expect(report.groups[0].share).toBeCloseTo(0.007 / total, 9);
    expect(
      report.items.map((i) => [
        i.name,
        i.group,
        i.calls,
        Number(i.costUsd.toFixed(6)),
      ]),
    ).toEqual([
      ["삼성전자", "stock", 2, 0.007],
      ["공용(토픽 갱신·시장 이슈·종목 찾기 등)", "shared", 1, 0.005],
      ["항목 기록 없음(11단계 전)", "unknown", 1, 0.003],
      ["부동산 시장", "topic", 2, 0.002],
      ["한국 증시", "market", 1, 0.002],
      ["자동 토픽(2개)", "topic", 2, 0.001],
    ]);
    expect(report.items.reduce((sum, item) => sum + item.share, 0)).toBeCloseTo(
      1,
      9,
    );
  });

  it("지운 항목도 이름과 '삭제함' 표시로 남는다", () => {
    const db = testDb();
    const user = addUserTopic(db, "중동 정세", NOW);
    if (!user.ok) throw new Error("주제");
    ensureDefaultMarket(db, NOW);
    // 증시가 켜진 뒤에 항목이 생긴다.
    const [lens] = syncTopicLenses(
      db,
      db.select().from(topics).where(eq(topics.id, user.topicId)).get()!,
      NOW,
    );
    recordUsage(db, {
      provider: "typesafe",
      model: "m",
      purpose: "article_evaluation",
      usage: { inputTokens: 1, outputTokens: 1, costUsd: 0.001 },
      at: NOW,
      stockId: lens,
    });
    deleteUserTopic(db, user.topicId, NOW);
    expect(monthUsageReport(db, NOW).items).toEqual([
      expect.objectContaining({ name: "중동 정세", deleted: true }),
    ]);
  });
});
