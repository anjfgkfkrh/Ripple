import type { Impact, Market, MatchKind } from "@/server/db/schema";

/** 판단 대상: 종목, 시장(한국·미국 증시), 주제(사용자 주제·자동 토픽, 증시 기준으로 판단) (11단계) */
export const JUDGE_TARGETS = ["stock", "market", "topic"] as const;
export type JudgeTarget = (typeof JUDGE_TARGETS)[number];

/** 판단 대상 항목. 시장·주제는 market이 기준 증시다. */
export type JudgeItem = {
  kind: JudgeTarget;
  name: string;
  /** 미국 종목의 공식 영문 이름 등. 한글 이름과 같으면 생략 */
  officialName: string | null;
  market: Market;
  exchange: string;
  code: string;
  /** 주제의 한 줄 설명(자동 토픽) */
  summary?: string | null;
};

/** 기사 하나가 항목 하나에 걸린 상황(Jev 입력, D-21) */
export type DecisionInput = {
  stock: JudgeItem;
  link: {
    kind: MatchKind;
    /** 종목에서 걸린 키워드까지의 경로(D-16) */
    path: string;
    /** 걸린 키워드가 윗단계와 이어지는 이유(키워드 연결일 때만) */
    reason: string | null;
  };
  article: {
    title: string;
    /** 피드가 준 요약 앞 300자 */
    summary: string | null;
  };
};

export type Decision = {
  /** 관련도 0~10 */
  relevance: number;
  relevanceConfidence: number;
  /** 영향도 0~10(주가·시장을 얼마나 움직일까, 11단계) */
  magnitude: number;
  magnitudeConfidence: number;
  impact: Impact;
  impactConfidence: number;
  impactProbabilities: Record<Impact, number>;
  /** 응답한 실제 모델 버전 */
  model: string;
  usage: { inputTokens: number; outputTokens: number };
};

/** 키워드 트리 후보나 토픽이 대상과 얼마나 이어지는지 묻는 입력(11단계) */
export type LeadInput = {
  stock: JudgeItem;
  subject: {
    type: "keyword" | "topic";
    /** 키워드 또는 토픽 이름 */
    text: string;
    /** 키워드: 대상에서 키워드까지의 경로 */
    path?: string;
    /** 토픽: 한 줄 설명과 검색어 */
    summary?: string;
    terms?: string[];
    /** 앞 단계(또는 대상)와 이어지는 이유 */
    reason: string | null;
  };
};

export type LeadRating = {
  relevance: number;
  relevanceConfidence: number;
  magnitude: number;
  magnitudeConfidence: number;
  model: string;
  usage: { inputTokens: number; outputTokens: number };
};

export type DeciderErrorKind =
  | "no_api_key"
  | "unauthorized"
  | "rate_limited"
  | "timeout"
  | "upstream"
  | "invalid_request";

export class DeciderError extends Error {
  constructor(
    message: string,
    readonly kind: DeciderErrorKind,
  ) {
    super(message);
    this.name = "DeciderError";
  }

  /** 다음 호출도 똑같이 실패할 오류(키 문제). 이번 회차 평가를 멈춘다. */
  get fatal(): boolean {
    return this.kind === "no_api_key" || this.kind === "unauthorized";
  }
}

/**
 * 기사 평가 인터페이스(SPEC 6.2 ai/decider). MVP는 TypeSafe 직접 호출,
 * 추후 OpenRouter를 거치는 구현으로 바꿀 수 있게 이 뒤에 둔다(CLAUDE.md).
 */
export interface Decider {
  readonly provider: "typesafe";
  /** 기사 평가. model은 설정 값(고정한 버전, D-23). 바꾸면 다음 평가부터 바로 쓴다. */
  decide(input: DecisionInput, model: string): Promise<Decision>;
  /** 키워드 트리 후보·토픽 평가(관련도·영향도, 11단계) */
  rate(input: LeadInput, model: string): Promise<LeadRating>;
}
