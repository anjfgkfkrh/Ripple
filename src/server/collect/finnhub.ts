import type { HttpClient } from "./http";

export const FINNHUB_BASE_URL = "https://finnhub.io/api/v1";

/** Finnhub 뉴스 한 건(공식 응답 필드 중 쓰는 것만) */
export type FinnhubNews = {
  id: number;
  headline: string;
  summary: string;
  /** Finnhub를 거치는 주소(누르면 원문으로 이동) */
  url: string;
  source: string;
  /** 초 단위 유닉스 시각 */
  datetime: number;
};

/** 미국 종목 현재가(`/quote`, 13단계). 무료 키로 받을 수 있다(2026-10-30 확인, 한국 종목은 403). */
export type FinnhubQuote = {
  /** 현재가 */
  price: number;
  /** 전일 종가 대비 */
  change: number;
  /** 전일 대비 등락률(%) */
  changePercent: number;
  /** 시세 시각 */
  quotedAt: Date;
};

export class FinnhubError extends Error {}

export type FinnhubClient = {
  /** 미국 종목 하나의 뉴스(종목에 바로 연결한다) */
  companyNews(symbol: string, from: Date, to: Date): Promise<FinnhubNews[]>;
  /** 시장 전체 뉴스(영어 키워드로 종목에 건다) */
  generalNews(): Promise<FinnhubNews[]>;
};

export type FinnhubQuoteClient = {
  /** 값이 없으면(현재가 0) null. 무료 키로 못 받는 티커(모르는 티커·한국 종목)는 403으로 던진다(2026-10-30 확인) */
  quote(symbol: string): Promise<FinnhubQuote | null>;
};

export function createFinnhubClient(options: {
  http: HttpClient;
  apiKey: string;
  baseUrl?: string;
}): FinnhubClient & FinnhubQuoteClient {
  const base = (options.baseUrl ?? FINNHUB_BASE_URL).replace(/\/+$/, "");

  async function get(path: string): Promise<unknown> {
    const response = await options.http.get(`${base}${path}`, {
      "X-Finnhub-Token": options.apiKey,
    });
    if (response.status === 401 || response.status === 403) {
      throw new FinnhubError("Finnhub 키가 맞지 않거나 권한이 없습니다.");
    }
    if (response.status === 429) {
      throw new FinnhubError("Finnhub 호출 한도를 넘었습니다.");
    }
    if (response.status !== 200) {
      throw new FinnhubError(`Finnhub 응답 오류(HTTP ${response.status})`);
    }
    let body: unknown;
    try {
      body = JSON.parse(response.text);
    } catch {
      throw new FinnhubError("Finnhub 응답을 읽지 못했습니다.");
    }
    return body;
  }

  async function news(path: string): Promise<FinnhubNews[]> {
    const body = await get(path);
    if (!Array.isArray(body))
      throw new FinnhubError("Finnhub 응답 형식이 다릅니다.");
    return body.filter(
      (n): n is FinnhubNews =>
        typeof n === "object" &&
        n !== null &&
        typeof (n as FinnhubNews).headline === "string" &&
        typeof (n as FinnhubNews).url === "string" &&
        typeof (n as FinnhubNews).datetime === "number",
    );
  }

  return {
    companyNews: (symbol, from, to) =>
      news(
        `/company-news?symbol=${encodeURIComponent(symbol)}&from=${day(from)}&to=${day(to)}`,
      ),
    generalNews: () => news("/news?category=general"),
    async quote(symbol) {
      const body = await get(`/quote?symbol=${encodeURIComponent(symbol)}`);
      const q = (
        typeof body === "object" && body !== null ? body : {}
      ) as Record<string, unknown>;
      const numbers = [q.c, q.d, q.dp, q.t];
      if (!numbers.every((n) => typeof n === "number" || n === null)) {
        throw new FinnhubError("Finnhub 응답 형식이 다릅니다.");
      }
      if (typeof q.c !== "number" || q.c <= 0 || typeof q.t !== "number") {
        return null;
      }
      return {
        price: q.c,
        change: typeof q.d === "number" ? q.d : 0,
        changePercent: typeof q.dp === "number" ? q.dp : 0,
        quotedAt: new Date(q.t * 1000),
      };
    },
  };
}

/** Finnhub는 날짜(YYYY-MM-DD, UTC)로 기간을 받는다. */
function day(date: Date): string {
  return date.toISOString().slice(0, 10);
}
