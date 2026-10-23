import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../tests/db";
import { openDb, type Db } from "./db/client";
import { settings } from "./db/schema";
import { readSetting, SETTING_DEFAULTS, writeSetting } from "./settings";

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

describe("settings", () => {
  it("저장된 값이 없으면 기본값(D-02 모델, D-04 임시 $5)", () => {
    const db = testDb();
    expect(SETTING_DEFAULTS["llm.model"]).toBe("anthropic/claude-haiku-5.5");
    expect(readSetting(db, "llm.model")).toBe("anthropic/claude-haiku-5.5");
    expect(readSetting(db, "budget.monthlyUsd")).toBe(5);
  });

  it("쓴 값을 읽고, 다시 쓰면 바뀐다", () => {
    const db = testDb();
    writeSetting(db, "llm.model", "google/gemini-3.8-flash");
    expect(readSetting(db, "llm.model")).toBe("google/gemini-3.8-flash");
    writeSetting(db, "llm.model", "openai/gpt-5.6-luna");
    expect(readSetting(db, "llm.model")).toBe("openai/gpt-5.6-luna");
  });

  it("저장된 값의 형식이 틀리면 기본값", () => {
    const db = testDb();
    db.insert(settings)
      .values({ key: "budget.monthlyUsd", value: '"많이"' })
      .run();
    expect(readSetting(db, "budget.monthlyUsd")).toBe(5);
    db.update(settings).set({ value: "not json" }).run();
    expect(readSetting(db, "budget.monthlyUsd")).toBe(5);
  });

  it("목록 설정(토픽 갱신 시각)은 글자 목록일 때만 쓰고 아니면 기본값(11단계)", () => {
    const db = testDb();
    expect(readSetting(db, "topics.times")).toEqual([]);
    writeSetting(db, "topics.times", ["08:30", "15:40"]);
    expect(readSetting(db, "topics.times")).toEqual(["08:30", "15:40"]);
    db.update(settings).set({ value: "[8, 30]" }).run();
    expect(readSetting(db, "topics.times")).toEqual([]);
    db.update(settings).set({ value: '"08:30"' }).run();
    expect(readSetting(db, "topics.times")).toEqual([]);
  });
});
