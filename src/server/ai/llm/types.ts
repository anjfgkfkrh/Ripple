import type * as z from "zod";

/** 생성형 LLM 인터페이스 (SPEC 6.2 ai/llm). MVP 구현은 OpenRouter. */
export interface LlmClient {
  readonly provider: "openrouter";
  /** 정해진 스키마에 맞는 JSON을 받는다. */
  generateObject<T>(
    request: StructuredRequest<T>,
  ): Promise<StructuredResult<T>>;
}

export type LlmMessage = { role: "system" | "user"; content: string };

export type StructuredRequest<T> = {
  model: string;
  messages: LlmMessage[];
  schema: z.ZodType<T>;
  /** 구조화 출력 스키마 이름(영문·숫자·밑줄) */
  schemaName: string;
  maxTokens: number;
};

export type LlmUsage = {
  inputTokens: number;
  outputTokens: number;
  /** 실제 청구액(달러) */
  costUsd: number;
};

export type StructuredResult<T> = { value: T; usage: LlmUsage; model: string };

export type LlmErrorKind =
  | "no_api_key"
  | "payment_required"
  | "rate_limited"
  | "timeout"
  | "upstream"
  | "invalid_output";

export class LlmError extends Error {
  constructor(
    message: string,
    readonly kind: LlmErrorKind,
    /** 응답을 받은 뒤 실패했으면 청구된 사용량(기록해야 함) */
    readonly usage?: LlmUsage,
  ) {
    super(message);
    this.name = "LlmError";
  }
}
