"use server";

import { refresh } from "next/cache";
import { requireSession } from "@/server/auth/guard";
import { MARKETS, type Market } from "@/server/db/schema";
import { MAX_QUERY_LENGTH, type StockCandidate } from "@/server/stocks/match";
import { describeStockError } from "@/server/stocks/messages";
import { webWorkerApi as workerApi } from "@/server/web-worker";

export type MatchState = {
  /** 검색할 때마다 늘어난다. 후보 목록을 새로 그리는 데 쓴다. */
  searchId: number;
  query: string;
  candidates: StockCandidate[];
  searched: boolean;
  error?: string;
};

/** 자유 입력 → 후보. LLM 호출과 사용량 기록은 worker가 한다(SPEC D-36). */
export async function matchStocksAction(
  previous: MatchState,
  formData: FormData,
): Promise<MatchState> {
  await requireSession();
  const query = String(formData.get("query") ?? "")
    .trim()
    .slice(0, MAX_QUERY_LENGTH);
  const next = {
    searchId: previous.searchId + 1,
    query,
    candidates: [],
    searched: true,
  };
  if (!query) return { ...next, error: "종목 이름이나 티커를 입력하세요." };

  const result = await workerApi().matchStocks(query);
  if (!result.ok) {
    return { ...next, error: describeStockError(result.error, result.body) };
  }
  return { ...next, candidates: result.data.candidates };
}

export type RegisterState = {
  status?: "registered" | "exists";
  error?: string;
};

export async function registerStockAction(
  _previous: RegisterState,
  formData: FormData,
): Promise<RegisterState> {
  await requireSession();
  const market = String(formData.get("market"));
  if (!(MARKETS as readonly string[]).includes(market)) {
    return { error: describeStockError("invalid_request") };
  }
  const result = await workerApi().registerStock({
    market: market as Market,
    code: String(formData.get("code") ?? ""),
    userInput: String(formData.get("userInput") ?? ""),
    nameKo: String(formData.get("nameKo") ?? "") || null,
  });
  if (!result.ok) {
    if (result.error === "already_registered") return { status: "exists" };
    return { error: describeStockError(result.error, result.body) };
  }
  refresh();
  return { status: "registered" };
}

export type DeleteState = { error?: string };

export async function deleteStockAction(
  _previous: DeleteState,
  formData: FormData,
): Promise<DeleteState> {
  await requireSession();
  const id = Number(formData.get("id"));
  if (!Number.isInteger(id) || id <= 0) {
    return { error: describeStockError("invalid_request") };
  }
  const result = await workerApi().deleteStock(id);
  // 이미 지워졌으면 목록만 새로 그리면 된다.
  if (!result.ok && result.error !== "not_found") {
    return { error: describeStockError(result.error, result.body) };
  }
  refresh();
  return {};
}

export type RegenerateState = { error?: string };

/** 키워드 다시 만들기: worker 대기열에 넣고 바로 돌아온다. 결과는 목록이 새로 읽어 보여 준다(D-37). */
export async function regenerateKeywordsAction(
  _previous: RegenerateState,
  formData: FormData,
): Promise<RegenerateState> {
  await requireSession();
  const id = Number(formData.get("id"));
  if (!Number.isInteger(id) || id <= 0) {
    return { error: describeStockError("invalid_request") };
  }
  const result = await workerApi().regenerateKeywords(id);
  if (!result.ok) {
    return { error: describeStockError(result.error, result.body) };
  }
  refresh();
  return {};
}
