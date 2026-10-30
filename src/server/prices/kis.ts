/**
 * 한국투자증권 KIS Open API로 한국 종목 현재가를 받는다(13단계, 2026-10-30 결정).
 * 주소·필드는 공식 예제(github.com/koreainvestment/open-trading-api, examples_llm/domestic_stock/inquire_price,
 * examples_user/kis_auth.py)를 따른다. 시세 조회만 하고 주문 API는 쓰지 않는다.
 */
export type KisEnv = "demo" | "real";

export const KIS_BASE_URLS: Record<KisEnv, string> = {
  demo: "https://openapivts.koreainvestment.com:29443",
  real: "https://openapi.koreainvestment.com:9443",
};

/** 주식현재가 시세 */
const PRICE_PATH = "/uapi/domestic-stock/v1/quotations/inquire-price";
const PRICE_TR_ID = "FHKST01010100";
/** 통합(KRX + 넥스트레이드, 2026-10-30 결정). J는 KRX, NX는 넥스트레이드만 */
export const KIS_MARKET_CODE = "UN";
/** 토큰은 1분에 1번만 발급된다(공식 예제 README). 실패하면 이만큼 기다린 뒤 다시 받는다. */
export const TOKEN_RETRY_MS = 61 * 1000;
/** 만료 이만큼 전에 새로 받는다. */
const TOKEN_REFRESH_MARGIN_MS = 10 * 60 * 1000;

export type KisQuote = {
  /** 현재가(원) */
  price: number;
  /** 기준가(전일 종가) 대비 */
  change: number;
  /** 기준가 대비 등락률(%) */
  changePercent: number;
};

export class KisError extends Error {}

export type KisClient = {
  /** 종목코드(6자리) 하나의 현재가 */
  domesticQuote(code: string): Promise<KisQuote>;
};

export function createKisClient(options: {
  appKey: string;
  appSecret: string;
  env: KisEnv;
  /** 테스트용 가짜 서버 주소. 없으면 env의 공식 주소 */
  baseUrl?: string;
  fetchImpl?: typeof fetch;
  now?: () => Date;
  /** 요청 시작 간격(호출 한도 대비) */
  minIntervalMs?: number;
  sleep?: (ms: number) => Promise<void>;
  timeoutMs?: number;
}): KisClient {
  const base = (options.baseUrl ?? KIS_BASE_URLS[options.env]).replace(
    /\/+$/,
    "",
  );
  const fetchImpl = options.fetchImpl ?? fetch;
  const now = options.now ?? (() => new Date());
  const minIntervalMs = options.minIntervalMs ?? 1100;
  const sleep =
    options.sleep ??
    ((ms: number) => new Promise((resolve) => setTimeout(resolve, ms)));
  const timeoutMs = options.timeoutMs ?? 20_000;

  // 토큰은 worker 메모리에만 둔다(다시 시작하면 새로 받음, 2026-10-30 결정).
  let token: { value: string; expiresAt: number } | null = null;
  let tokenFailedAt = -Infinity;
  let nextAt = 0;

  async function request(
    url: string,
    init: RequestInit,
  ): Promise<{ status: number; body: Record<string, unknown> }> {
    const wait = nextAt - now().getTime();
    nextAt = Math.max(now().getTime(), nextAt) + minIntervalMs;
    if (wait > 0) await sleep(wait);
    let response: Response;
    try {
      response = await fetchImpl(url, {
        ...init,
        signal: AbortSignal.timeout(timeoutMs),
      });
    } catch (error) {
      throw new KisError(
        error instanceof Error && error.name === "TimeoutError"
          ? "KIS 응답이 늦어 중단했습니다."
          : `KIS에 연결하지 못했습니다: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
    let body: unknown;
    try {
      body = await response.json();
    } catch {
      throw new KisError(
        `KIS 응답을 읽지 못했습니다(HTTP ${response.status}).`,
      );
    }
    return {
      status: response.status,
      body:
        typeof body === "object" && body !== null
          ? (body as Record<string, unknown>)
          : {},
    };
  }

  async function accessToken(): Promise<string> {
    const at = now().getTime();
    if (token && token.expiresAt - TOKEN_REFRESH_MARGIN_MS > at) {
      return token.value;
    }
    if (at - tokenFailedAt < TOKEN_RETRY_MS) {
      throw new KisError("KIS 토큰 발급이 방금 실패해 1분 뒤 다시 받습니다.");
    }
    const { status, body } = await request(`${base}/oauth2/tokenP`, {
      method: "POST",
      headers: { "content-type": "application/json; charset=utf-8" },
      body: JSON.stringify({
        grant_type: "client_credentials",
        appkey: options.appKey,
        appsecret: options.appSecret,
      }),
    });
    const value = body.access_token;
    if (status !== 200 || typeof value !== "string" || !value) {
      tokenFailedAt = at;
      const reason =
        typeof body.error_description === "string"
          ? body.error_description
          : `HTTP ${status}`;
      throw new KisError(`KIS 토큰을 받지 못했습니다: ${reason}`);
    }
    token = { value, expiresAt: at + tokenLifetimeMs(body) };
    return value;
  }

  async function domesticQuote(
    code: string,
    retried = false,
  ): Promise<KisQuote> {
    const params = new URLSearchParams({
      FID_COND_MRKT_DIV_CODE: KIS_MARKET_CODE,
      FID_INPUT_ISCD: code,
    });
    const { status, body } = await request(`${base}${PRICE_PATH}?${params}`, {
      method: "GET",
      headers: {
        "content-type": "application/json; charset=utf-8",
        authorization: `Bearer ${await accessToken()}`,
        appkey: options.appKey,
        appsecret: options.appSecret,
        tr_id: PRICE_TR_ID,
        custtype: "P",
      },
    });
    if (status !== 200 || body.rt_cd !== "0") {
      const msgCode = typeof body.msg_cd === "string" ? body.msg_cd : "";
      // EGW001xx는 토큰 문제(만료·잘못된 토큰). 새 토큰으로 한 번만 다시 묻는다.
      if (msgCode.startsWith("EGW001") && !retried) {
        token = null;
        return domesticQuote(code, true);
      }
      const message = typeof body.msg1 === "string" ? body.msg1.trim() : "";
      throw new KisError(
        `KIS 시세 오류(${[`HTTP ${status}`, msgCode].filter(Boolean).join(", ")})${message ? `: ${message}` : ""}`,
      );
    }
    return parseQuote(body.output);
  }

  return { domesticQuote: (code) => domesticQuote(code) };
}

/** 응답의 expires_in(초)을 쓰고, 없으면 하루(공식 예제 주석)로 본다. */
function tokenLifetimeMs(body: Record<string, unknown>): number {
  const seconds = Number(body.expires_in);
  return Number.isFinite(seconds) && seconds > 0
    ? seconds * 1000
    : 24 * 60 * 60 * 1000;
}

/** 현재가(stck_prpr)와 기준가(stck_sdpr)로 대비·등락률을 계산한다(부호 칸을 해석하지 않아도 되게). */
export function parseQuote(output: unknown): KisQuote {
  const row = (
    typeof output === "object" && output !== null ? output : {}
  ) as Record<string, unknown>;
  const price = Number(row.stck_prpr);
  const base = Number(row.stck_sdpr);
  if (!(price > 0) || !(base > 0)) {
    throw new KisError("KIS 시세 응답에 현재가·기준가가 없습니다.");
  }
  const change = price - base;
  return {
    price,
    change,
    changePercent: Math.round((change / base) * 10000) / 100,
  };
}
