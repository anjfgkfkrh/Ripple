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

export class FinnhubError extends Error {}

export type FinnhubClient = {
  /** 미국 종목 하나의 뉴스(종목에 바로 연결한다) */
  companyNews(symbol: string, from: Date, to: Date): Promise<FinnhubNews[]>;
  /** 시장 전체 뉴스(영어 키워드로 종목에 건다) */
  generalNews(): Promise<FinnhubNews[]>;
};

export function createFinnhubClient(options: {
  http: HttpClient;
  apiKey: string;
  baseUrl?: string;
}): FinnhubClient {
  const base = (options.baseUrl ?? FINNHUB_BASE_URL).replace(/\/+$/, "");

  async function get(path: string): Promise<FinnhubNews[]> {
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
      get(
        `/company-news?symbol=${encodeURIComponent(symbol)}&from=${day(from)}&to=${day(to)}`,
      ),
    generalNews: () => get("/news?category=general"),
  };
}

/** Finnhub는 날짜(YYYY-MM-DD, UTC)로 기간을 받는다. */
function day(date: Date): string {
  return date.toISOString().slice(0, 10);
}
