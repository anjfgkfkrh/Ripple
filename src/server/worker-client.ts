/** web → worker 내부 API 호출 (SPEC D-36: 쓰기와 AI 호출은 worker가 맡는다) */
import type { Market } from "@/server/db/schema";
import type { StockCandidate } from "@/server/stocks/match";
import type { Stock } from "@/server/stocks/repository";

export type WorkerHealth =
  | { ok: true; listings?: { KR: number; US: number } }
  | { ok: false; reason: string };

export async function fetchWorkerHealth(
  workerUrl: string,
  timeoutMs = 2000,
): Promise<WorkerHealth> {
  try {
    const response = await fetch(new URL("/health", workerUrl), {
      signal: AbortSignal.timeout(timeoutMs),
    });
    if (!response.ok) {
      return { ok: false, reason: `HTTP ${response.status}` };
    }
    const body = (await response.json()) as {
      status?: unknown;
      listings?: { KR?: unknown; US?: unknown };
    };
    if (body.status !== "ok") {
      return { ok: false, reason: "응답 형식이 올바르지 않습니다." };
    }
    const { KR, US } = body.listings ?? {};
    return typeof KR === "number" && typeof US === "number"
      ? { ok: true, listings: { KR, US } }
      : { ok: true };
  } catch (error) {
    return { ok: false, reason: describeError(error) };
  }
}

/** 실패하면 worker가 준 오류 코드(예: budget_exceeded)와 본문을 그대로 돌려준다. */
export type WorkerResult<T> =
  | { ok: true; status: number; data: T }
  | {
      ok: false;
      status: number;
      error: string;
      body: Record<string, unknown>;
    };

export type WorkerApi = {
  matchStocks(
    query: string,
  ): Promise<WorkerResult<{ candidates: StockCandidate[] }>>;
  registerStock(input: {
    market: Market;
    code: string;
    userInput: string;
    nameKo: string | null;
  }): Promise<WorkerResult<{ status: "created" | "restored"; stock: Stock }>>;
  deleteStock(id: number): Promise<WorkerResult<{ ok: true }>>;
  /** 키워드 생성을 대기열에 넣는다. 결과는 기다리지 않는다(D-37). */
  regenerateKeywords(id: number): Promise<WorkerResult<{ status: "queued" }>>;
  /** 기사 수집을 시작한다. 결과는 기다리지 않는다(이미 도는 중이면 already_running). */
  startCollection(): Promise<WorkerResult<{ runId: number }>>;
  /** 설정 화면 값을 저장한다(worker가 검사·저장, 모델 ID는 OpenRouter 목록으로 확인). */
  updateSettings(
    values: Record<string, string>,
  ): Promise<WorkerResult<{ ok: true; changed: string[] }>>;
  /** 저장한 깊이와 다른 깊이로 만든 종목의 키워드를 다시 만든다. 결과는 기다리지 않는다. */
  rebuildKeywords(): Promise<WorkerResult<{ count: number }>>;
};

export function createWorkerApi(options: {
  workerUrl: string;
  token: string;
}): WorkerApi {
  const call = async <T>(
    path: string,
    init: {
      method: "POST" | "PUT" | "DELETE";
      body?: unknown;
      timeoutMs: number;
    },
  ): Promise<WorkerResult<T>> => {
    try {
      const response = await fetch(new URL(path, options.workerUrl), {
        method: init.method,
        headers: {
          Authorization: `Bearer ${options.token}`,
          "Content-Type": "application/json",
        },
        body: init.body === undefined ? undefined : JSON.stringify(init.body),
        signal: AbortSignal.timeout(init.timeoutMs),
      });
      const body = (await response.json().catch(() => ({}))) as Record<
        string,
        unknown
      >;
      if (!response.ok) {
        return {
          ok: false,
          status: response.status,
          error:
            typeof body.error === "string"
              ? body.error
              : `http_${response.status}`,
          body,
        };
      }
      return { ok: true, status: response.status, data: body as T };
    } catch (error) {
      const timedOut = error instanceof Error && error.name === "TimeoutError";
      return {
        ok: false,
        status: 0,
        error: timedOut ? "worker_timeout" : "worker_unreachable",
        body: {},
      };
    }
  };

  return {
    // LLM 응답을 기다리므로 넉넉하게 기다린다(worker의 OpenRouter 제한 30초보다 길게).
    matchStocks: (query) =>
      call("/stocks/match", {
        method: "POST",
        body: { query },
        timeoutMs: 45_000,
      }),
    registerStock: (input) =>
      call("/stocks", { method: "POST", body: input, timeoutMs: 10_000 }),
    deleteStock: (id) =>
      call(`/stocks/${id}`, { method: "DELETE", timeoutMs: 10_000 }),
    regenerateKeywords: (id) =>
      call(`/stocks/${id}/keywords`, { method: "POST", timeoutMs: 10_000 }),
    startCollection: () =>
      call("/collect", { method: "POST", timeoutMs: 10_000 }),
    // 모델을 바꾸면 worker가 OpenRouter 목록(최대 15초)을 확인하므로 넉넉하게 기다린다.
    updateSettings: (values) =>
      call("/settings", { method: "PUT", body: values, timeoutMs: 30_000 }),
    rebuildKeywords: () =>
      call("/keywords/rebuild", { method: "POST", timeoutMs: 10_000 }),
  };
}

function describeError(error: unknown): string {
  if (error instanceof Error) {
    const cause =
      error.cause instanceof Error ? ` (${error.cause.message})` : "";
    return `${error.message}${cause}`;
  }
  return String(error);
}
