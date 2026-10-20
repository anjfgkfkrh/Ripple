import type { Market } from "@/server/db/schema";

export type ListingRow = {
  market: Market;
  code: string;
  name: string;
  exchange: string;
  isEtf: boolean;
};

/** KIND 시장구분 → 거래소 */
const KR_EXCHANGES: Record<string, string> = {
  유가: "KOSPI",
  코스닥: "KOSDAQ",
  코넥스: "KONEX",
};

/**
 * otherlisted.txt의 Exchange 값 (NASDAQ Trader 정의 페이지 기준).
 * NYSE MKT는 지금의 NYSE American, BATS는 지금의 Cboe BZX다.
 */
const US_EXCHANGES: Record<string, string> = {
  A: "NYSE American",
  N: "NYSE",
  P: "NYSE Arca",
  Z: "Cboe BZX",
  V: "IEX",
};

/** 한국 종목코드: 6자리. 2024년부터 영문이 섞인 코드도 있다(예: 0158S0). */
export const KR_CODE = /^[0-9A-Z]{6}$/;

/** KIND 상장법인목록. 확장자는 .xls지만 실제로는 HTML 표다(EUC-KR은 받는 쪽에서 풂). */
export function parseKindList(html: string): ListingRow[] {
  const rows = [...html.matchAll(/<tr[^>]*>([\s\S]*?)<\/tr>/gi)].map((row) =>
    [...row[1].matchAll(/<t[dh][^>]*>([\s\S]*?)<\/t[dh]>/gi)].map((cell) =>
      decodeHtml(cell[1].replace(/<[^>]+>/g, "")).trim(),
    ),
  );
  const [header = [], ...body] = rows;
  const nameAt = header.indexOf("회사명");
  const marketAt = header.indexOf("시장구분");
  const codeAt = header.indexOf("종목코드");
  if (nameAt < 0 || marketAt < 0 || codeAt < 0) {
    throw new Error("KIND 상장법인목록의 형식이 바뀌었습니다(열 이름).");
  }
  return body.flatMap((cells): ListingRow[] => {
    const code = (cells[codeAt] ?? "").toUpperCase();
    const name = cells[nameAt] ?? "";
    if (!KR_CODE.test(code) || !name) return [];
    const market = cells[marketAt] ?? "";
    return [
      {
        market: "KR",
        code,
        name,
        exchange: KR_EXCHANGES[market] ?? market,
        isEtf: false,
      },
    ];
  });
}

/** NASDAQ Trader nasdaqlisted.txt (NASDAQ 상장) */
export function parseNasdaqListed(text: string): ListingRow[] {
  return parsePipeFile(text, {
    symbol: "Symbol",
    exchangeOf: () => "NASDAQ",
  });
}

/** NASDAQ Trader otherlisted.txt (NYSE 등 다른 거래소 상장) */
export function parseOtherListed(text: string): ListingRow[] {
  return parsePipeFile(text, {
    symbol: "ACT Symbol",
    exchangeOf: (row) => US_EXCHANGES[row.Exchange] ?? row.Exchange,
  });
}

function parsePipeFile(
  text: string,
  options: {
    symbol: string;
    exchangeOf: (row: Record<string, string>) => string;
  },
): ListingRow[] {
  const lines = text.split(/\r?\n/).filter((line) => line.trim() !== "");
  const header = (lines[0] ?? "").split("|");
  for (const column of [options.symbol, "Security Name", "ETF", "Test Issue"]) {
    if (!header.includes(column)) {
      throw new Error(
        `NASDAQ 종목 목록의 형식이 바뀌었습니다(${column} 열 없음).`,
      );
    }
  }
  return lines.slice(1).flatMap((line): ListingRow[] => {
    // 마지막 줄은 "File Creation Time: ..." 이다.
    if (line.startsWith("File Creation Time")) return [];
    const values = line.split("|");
    const row = Object.fromEntries(
      header.map((key, i) => [key, values[i] ?? ""]),
    );
    const code = row[options.symbol].trim().toUpperCase();
    const name = row["Security Name"].replace(/\s+/g, " ").trim();
    if (!code || !name || row["Test Issue"] === "Y") return [];
    return [
      {
        market: "US",
        code,
        name,
        exchange: options.exchangeOf(row),
        isEtf: row.ETF === "Y",
      },
    ];
  });
}

function decodeHtml(text: string): string {
  return text
    .replace(/&#(\d+);/g, (_, n: string) => String.fromCodePoint(Number(n)))
    .replace(/&#x([0-9a-f]+);/gi, (_, n: string) =>
      String.fromCodePoint(parseInt(n, 16)),
    )
    .replace(/&nbsp;/g, " ")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;|&apos;/g, "'")
    .replace(/&amp;/g, "&");
}
