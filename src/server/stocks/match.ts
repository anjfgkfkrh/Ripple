import * as z from "zod";
import type { LlmClient, LlmMessage } from "@/server/ai/llm/types";
import { generateWithBudget } from "@/server/budget/usage";
import type { Db } from "@/server/db/client";
import { MARKETS, type Market } from "@/server/db/schema";
import { findListing, findListingByName } from "@/server/listings/lookup";
import { listingCount } from "@/server/listings/sync";
import { readSetting } from "@/server/settings";
import { isActiveStock, MAX_NAME_KO_LENGTH } from "./repository";

/** 후보는 최대 3개 */
export const MAX_CANDIDATES = 3;
export const MAX_QUERY_LENGTH = 50;

/** LLM 출력 형식. 같은 스키마로 OpenRouter 구조화 출력을 요청하고 응답을 검증한다. */
export const matchOutputSchema = z.object({
  candidates: z.array(
    z.object({
      market: z
        .enum(MARKETS)
        .describe("KR: 한국 거래소 상장, US: 미국 거래소 상장"),
      code: z.string().describe("KR은 6자리 종목코드, US는 티커"),
      name: z.string().describe("정식 회사 이름. KR은 한글, US는 영문"),
      nameKo: z.string().describe("한국에서 흔히 부르는 한글 이름"),
    }),
  ),
});
export type LlmCandidate = z.infer<
  typeof matchOutputSchema
>["candidates"][number];

export type StockCandidate = {
  market: Market;
  code: string;
  name: string;
  nameKo: string | null;
  exchange: string;
  isEtf: boolean;
  alreadyRegistered: boolean;
};

export class ListingsUnavailableError extends Error {
  constructor() {
    super("공식 종목 목록을 아직 받지 못했습니다.");
    this.name = "ListingsUnavailableError";
  }
}

export function buildMatchMessages(query: string): LlmMessage[] {
  return [
    {
      role: "system",
      content: [
        "너는 한국·미국 상장 주식의 종목을 찾아 주는 도우미다.",
        "사용자가 적은 종목명·줄임말·별명·영문명·티커·종목코드를 실제 상장 종목으로 바꿔, 가능성이 높은 순서로 최대 3개 후보를 낸다.",
        "- market: 한국 거래소(KOSPI·KOSDAQ·KONEX) 상장이면 KR, 미국 거래소(NASDAQ·NYSE 등) 상장이면 US.",
        "- code: KR은 6자리 종목코드, US는 티커.",
        "- name: 정식 회사 이름. KR은 한글, US는 영문.",
        "- nameKo: 한국에서 흔히 부르는 한글 이름(예: NVIDIA → 엔비디아).",
        "- 확실한 종목이 하나면 하나만 낸다. 어떤 종목인지 모르겠으면 빈 목록을 낸다. 없는 종목을 지어내지 않는다.",
      ].join("\n"),
    },
    { role: "user", content: query },
  ];
}

/**
 * LLM 후보를 공식 목록과 대조한다(D-12). 목록에 없는 후보는 버리고 이름·거래소는 목록 값을 쓴다.
 * 한국 종목은 코드가 틀려도 정식 이름이 목록과 같으면 목록의 코드를 쓴다.
 */
export function verifyCandidates(
  db: Db,
  candidates: LlmCandidate[],
): StockCandidate[] {
  const verified: StockCandidate[] = [];
  const seen = new Set<string>();
  for (const candidate of candidates) {
    const listing =
      findListing(db, candidate.market, candidate.code) ??
      (candidate.market === "KR"
        ? findListingByName(db, "KR", candidate.name)
        : undefined);
    if (!listing) continue;
    const key = `${listing.market}:${listing.code}`;
    if (seen.has(key)) continue;
    seen.add(key);
    verified.push({
      market: listing.market,
      code: listing.code,
      name: listing.name,
      nameKo:
        listing.market === "KR"
          ? listing.name
          : candidate.nameKo.trim().slice(0, MAX_NAME_KO_LENGTH) || null,
      exchange: listing.exchange,
      isEtf: listing.isEtf,
      alreadyRegistered: isActiveStock(db, listing.market, listing.code),
    });
    if (verified.length === MAX_CANDIDATES) break;
  }
  return verified;
}

/** 자유 입력 → 후보(최대 3개). AI 호출은 예산 검사·사용량 기록을 거친다. */
export async function matchStocks(
  db: Db,
  llm: LlmClient,
  query: string,
  options: { now?: () => Date } = {},
): Promise<StockCandidate[]> {
  if (listingCount(db, "KR") === 0 && listingCount(db, "US") === 0) {
    throw new ListingsUnavailableError();
  }
  const { value } = await generateWithBudget(
    db,
    llm,
    {
      model: readSetting(db, "llm.model"),
      messages: buildMatchMessages(query.trim()),
      schema: matchOutputSchema,
      schemaName: "stock_candidates",
      // 추론(생각) 토큰도 이 한도를 함께 쓴다(Claude 5.5 계열). 최대 비용은 1회 $0.001 수준.
      maxTokens: 2000,
    },
    { purpose: "stock_match", now: options.now },
  );
  return verifyCandidates(db, value.candidates);
}
