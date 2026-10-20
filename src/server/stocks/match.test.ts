import { afterEach, describe, expect, it } from "vitest";
import { createTestDbFile } from "../../../tests/db";
import { fakeLlm, seedListings } from "../../../tests/fixtures";
import { openDb, type Db } from "../db/client";
import { usageLogs } from "../db/schema";
import { writeSetting } from "../settings";
import {
  ListingsUnavailableError,
  MAX_CANDIDATES,
  matchStocks,
  verifyCandidates,
  type LlmCandidate,
} from "./match";
import { registerStock } from "./repository";

const opened: Db[] = [];
const cleanups: (() => void)[] = [];
function testDb({ seeded = true } = {}): Db {
  const file = createTestDbFile();
  cleanups.push(file.cleanup);
  const db = openDb(file.path, "readwrite");
  opened.push(db);
  if (seeded) seedListings(db);
  return db;
}
afterEach(() => {
  opened.splice(0).forEach((db) => db.$client.close());
  cleanups.splice(0).forEach((cleanup) => cleanup());
});

const kr = (code: string, name: string): LlmCandidate => ({
  market: "KR",
  code,
  name,
  nameKo: name,
});
const us = (code: string, name: string, nameKo: string): LlmCandidate => ({
  market: "US",
  code,
  name,
  nameKo,
});

describe("verifyCandidates (SPEC D-12: 공식 목록과 대조)", () => {
  it("목록에 있는 후보만 남기고, 이름·거래소는 목록 값을 쓴다", () => {
    const db = testDb();
    const result = verifyCandidates(db, [
      kr("005930", "삼성전자(주)"),
      us("NVDA", "Nvidia", "엔비디아"),
      kr("999999", "없는회사"),
    ]);
    expect(result).toEqual([
      {
        market: "KR",
        code: "005930",
        name: "삼성전자",
        nameKo: "삼성전자",
        exchange: "KOSPI",
        isEtf: false,
        alreadyRegistered: false,
      },
      {
        market: "US",
        code: "NVDA",
        name: "NVIDIA Corporation - Common Stock",
        nameKo: "엔비디아",
        exchange: "NASDAQ",
        isEtf: false,
        alreadyRegistered: false,
      },
    ]);
  });

  it("한국 종목은 코드가 틀려도 정식 이름이 맞으면 목록의 코드로 바로잡는다", () => {
    const db = testDb();
    const [candidate] = verifyCandidates(db, [kr("000000", "SK하이닉스")]);
    expect(candidate?.code).toBe("000660");
  });

  it("코드 표기 차이(A 접두어, 하이픈 클래스)를 맞춘다", () => {
    const db = testDb();
    const result = verifyCandidates(db, [
      kr("A005930", "삼성전자"),
      us("BRK-B", "Berkshire Hathaway", "버크셔 해서웨이"),
    ]);
    expect(result.map((c) => c.code)).toEqual(["005930", "BRK.B"]);
  });

  it("같은 종목은 한 번만, 최대 3개까지", () => {
    const db = testDb();
    const result = verifyCandidates(db, [
      kr("005930", "삼성전자"),
      kr("005930", "삼성전자"),
      kr("000660", "SK하이닉스"),
      kr("000720", "현대건설"),
      kr("035720", "카카오"),
    ]);
    expect(result).toHaveLength(MAX_CANDIDATES);
    expect(result.map((c) => c.code)).toEqual(["005930", "000660", "000720"]);
  });

  it("이미 등록한 종목은 표시한다", () => {
    const db = testDb();
    registerStock(db, { market: "KR", code: "005930", userInput: "삼전" });
    expect(
      verifyCandidates(db, [kr("005930", "삼성전자")])[0]?.alreadyRegistered,
    ).toBe(true);
  });
});

describe("matchStocks", () => {
  it("설정의 모델로 부르고, 사용량을 stock_match로 기록한다", async () => {
    const db = testDb();
    writeSetting(db, "llm.model", "google/gemini-3.8-flash");
    const llm = fakeLlm(() => ({ candidates: [kr("005930", "삼성전자")] }));

    const candidates = await matchStocks(db, llm, "  삼전 ");

    expect(candidates.map((c) => c.name)).toEqual(["삼성전자"]);
    expect(llm.calls[0]?.model).toBe("google/gemini-3.8-flash");
    expect(llm.calls[0]?.messages.at(-1)).toEqual({
      role: "user",
      content: "삼전",
    });
    expect(db.select().from(usageLogs).all()[0]?.purpose).toBe("stock_match");
  });

  it("공식 목록이 하나도 없으면 LLM을 부르지 않고 실패한다", async () => {
    const db = testDb({ seeded: false });
    const llm = fakeLlm(() => ({ candidates: [] }));
    await expect(matchStocks(db, llm, "삼전")).rejects.toBeInstanceOf(
      ListingsUnavailableError,
    );
    expect(llm.calls).toHaveLength(0);
  });
});
