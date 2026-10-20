import {
  createServer,
  type IncomingMessage,
  type Server,
  type ServerResponse,
} from "node:http";
import * as z from "zod";
import {
  LlmError,
  type LlmClient,
  type LlmErrorKind,
} from "@/server/ai/llm/types";
import { isAuthorizedBearer } from "@/server/auth/worker-token";
import { BudgetExceededError } from "@/server/budget/usage";
import type { CollectionRunner } from "@/server/collect/runner";
import type { Db } from "@/server/db/client";
import type { KeywordQueue } from "@/server/keywords/queue";
import { MARKETS } from "@/server/db/schema";
import { listingCount } from "@/server/listings/sync";
import { log, logError } from "@/server/log";
import {
  ListingsUnavailableError,
  matchStocks,
  MAX_QUERY_LENGTH,
} from "@/server/stocks/match";
import {
  deleteStock,
  isActiveStockId,
  registerStock,
} from "@/server/stocks/repository";

export type WorkerDeps = {
  db: Db;
  apiToken: string;
  llm: LlmClient;
  /** 등록 직후·다시 만들기 요청 때 키워드 생성을 맡긴다(D-37). */
  keywordQueue: KeywordQueue;
  /** 기사 수집(4단계). 6단계 전까지는 화면의 "지금 수집"으로만 돈다. */
  collector: CollectionRunner;
  now?: () => Date;
};

const MAX_BODY_BYTES = 16 * 1024;

const matchBody = z.object({
  query: z.string().trim().min(1).max(MAX_QUERY_LENGTH),
});

const registerBody = z.object({
  market: z.enum(MARKETS),
  code: z.string().trim().min(1).max(20),
  userInput: z.string().trim().min(1).max(MAX_QUERY_LENGTH),
  nameKo: z.string().max(100).nullish(),
});

const LLM_ERROR_STATUS: Record<LlmErrorKind, number> = {
  no_api_key: 503,
  payment_required: 402,
  rate_limited: 429,
  timeout: 504,
  upstream: 502,
  invalid_output: 502,
};

/**
 * worker 내부 API. compose 내부 네트워크에만 열고 호스트에는 공개하지 않는다.
 * DB 쓰기와 AI 호출은 모두 여기를 거친다(SPEC D-36). /health를 빼면 공유 토큰이 필요하다.
 */
export function createWorkerServer(deps: WorkerDeps): Server {
  return createServer((request, response) => {
    handle(deps, request, response).catch((error: unknown) => {
      logError("worker 내부 API 오류", error);
      if (!response.headersSent) sendJson(response, 500, { error: "internal" });
    });
  });
}

async function handle(
  deps: WorkerDeps,
  request: IncomingMessage,
  response: ServerResponse,
): Promise<void> {
  const { pathname } = new URL(request.url ?? "/", "http://worker");
  const method = request.method ?? "GET";

  if (method === "GET" && pathname === "/health") {
    try {
      deps.db.$client.prepare("SELECT 1").get();
      sendJson(response, 200, {
        status: "ok",
        db: "ok",
        listings: {
          KR: listingCount(deps.db, "KR"),
          US: listingCount(deps.db, "US"),
        },
      });
    } catch {
      sendJson(response, 503, { status: "error", db: "error" });
    }
    return;
  }

  if (!isAuthorizedBearer(request.headers.authorization, deps.apiToken)) {
    sendJson(response, 401, { error: "unauthorized" });
    return;
  }

  if (method === "POST" && pathname === "/stocks/match") {
    const body = await readBody(request, response, matchBody);
    if (!body) return;
    try {
      const candidates = await matchStocks(deps.db, deps.llm, body.query, {
        now: deps.now,
      });
      log(`종목 찾기 "${body.query}" → 후보 ${candidates.length}개`);
      sendJson(response, 200, { candidates });
    } catch (error) {
      sendMatchError(response, error);
    }
    return;
  }

  if (method === "POST" && pathname === "/stocks") {
    const body = await readBody(request, response, registerBody);
    if (!body) return;
    const result = registerStock(deps.db, body, deps.now?.());
    if (result.status === "not_listed") {
      sendJson(response, 404, { error: "not_listed" });
      return;
    }
    if (result.status === "exists") {
      sendJson(response, 409, {
        error: "already_registered",
        stock: result.stock,
      });
      return;
    }
    log(
      `종목 ${result.status === "created" ? "등록" : "복원"}: ${result.stock.name} (${result.stock.code})`,
    );
    // 키워드는 응답을 기다리게 하지 않고 따로 만든다(D-37).
    deps.keywordQueue.enqueue(result.stock.id);
    sendJson(response, result.status === "created" ? 201 : 200, result);
    return;
  }

  const keywordsPath = pathname.match(/^\/stocks\/(\d+)\/keywords$/);
  if (method === "POST" && keywordsPath) {
    const id = Number(keywordsPath[1]);
    if (!isActiveStockId(deps.db, id)) {
      sendJson(response, 404, { error: "not_found" });
      return;
    }
    deps.keywordQueue.enqueue(id);
    sendJson(response, 202, { status: "queued" });
    return;
  }

  if (method === "POST" && pathname === "/collect") {
    const runId = deps.collector.start("manual");
    if (runId === null) {
      sendJson(response, 409, {
        error: "already_running",
        runId: deps.collector.runningId(),
      });
      return;
    }
    log(`수집 시작: 회차 ${runId}(직접 실행)`);
    sendJson(response, 202, { runId });
    return;
  }

  const stockPath = pathname.match(/^\/stocks\/(\d+)$/);
  if (method === "DELETE" && stockPath) {
    const deleted = deleteStock(deps.db, Number(stockPath[1]), deps.now?.());
    if (deleted) log(`종목 삭제(숨김): id ${stockPath[1]}`);
    sendJson(
      response,
      deleted ? 200 : 404,
      deleted ? { ok: true } : { error: "not_found" },
    );
    return;
  }

  sendJson(response, 404, { error: "not_found" });
}

function sendMatchError(response: ServerResponse, error: unknown): void {
  if (error instanceof BudgetExceededError) {
    sendJson(response, 402, {
      error: "budget_exceeded",
      spentUsd: error.spentUsd,
      limitUsd: error.limitUsd,
    });
    return;
  }
  if (error instanceof ListingsUnavailableError) {
    sendJson(response, 503, { error: "listings_unavailable" });
    return;
  }
  if (error instanceof LlmError) {
    logError("종목 찾기 AI 호출 실패", error.message);
    sendJson(response, LLM_ERROR_STATUS[error.kind], {
      error: `llm_${error.kind}`,
    });
    return;
  }
  throw error;
}

/** JSON 본문을 읽고 검증한다. 실패하면 응답을 보내고 undefined를 돌려준다. */
async function readBody<T>(
  request: IncomingMessage,
  response: ServerResponse,
  schema: z.ZodType<T>,
): Promise<T | undefined> {
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const chunk of request) {
    size += (chunk as Buffer).length;
    if (size > MAX_BODY_BYTES) {
      sendJson(response, 413, { error: "body_too_large" });
      return undefined;
    }
    chunks.push(chunk as Buffer);
  }
  let json: unknown;
  try {
    json = JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } catch {
    json = undefined;
  }
  const parsed = schema.safeParse(json);
  if (!parsed.success) {
    sendJson(response, 400, { error: "invalid_request" });
    return undefined;
  }
  return parsed.data;
}

function sendJson(
  response: ServerResponse,
  status: number,
  body: unknown,
): void {
  response.writeHead(status, {
    "content-type": "application/json; charset=utf-8",
  });
  response.end(JSON.stringify(body));
}
