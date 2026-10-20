import type { Impact, Market, MatchKind } from "@/server/db/schema";

/** 기사 하나가 종목 하나에 걸린 상황(Jev 입력, D-21) */
export type DecisionInput = {
  stock: {
    name: string;
    /** 미국 종목의 공식 영문 이름 등. 한글 이름과 같으면 생략 */
    officialName: string | null;
    market: Market;
    exchange: string;
    code: string;
  };
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
  impact: Impact;
  impactConfidence: number;
  impactProbabilities: Record<Impact, number>;
  /** 응답한 실제 모델 버전 */
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
  /** model은 설정 값(고정한 버전, D-23). 바꾸면 다음 평가부터 바로 쓴다. */
  decide(input: DecisionInput, model: string): Promise<Decision>;
}
