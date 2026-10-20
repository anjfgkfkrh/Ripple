import {
  APIConnectionError,
  APIError,
  APITimeoutError,
  AuthenticationError,
  PermissionDeniedError,
  RateLimitError,
  TypeSafeClient,
} from "@typesafe-ai/sdk";
import { IMPACTS, type Impact } from "@/server/db/schema";
import {
  buildQuestions,
  buildState,
  RELEVANCE_SCALE,
  type QuestionLanguage,
} from "./questions";
import {
  DeciderError,
  type Decider,
  type Decision,
  type DecisionInput,
} from "./types";

export const TYPESAFE_BASE_URL = "https://api.typesafe.ai";

type Options = {
  apiKey: string | undefined;
  language: QuestionLanguage;
  baseUrl?: string;
  fetchImpl?: typeof fetch;
};

/**
 * TypeSafe 공식 SDK로 Jev를 부른다(SPEC 12장: MVP는 TypeScript SDK로 직접 호출).
 * 관련도와 호재/악재를 한 요청에 묻는다(입력은 한 번만 센다). 제한 시간(시도마다 10초)과
 * 재시도(429·5xx·연결 오류는 두 번 더)는 SDK 기본값을 쓴다(실측 응답은 최대 0.6초).
 */
export function createTypeSafeDecider(options: Options): Decider {
  const questions = buildQuestions(options.language);
  const client = options.apiKey
    ? new TypeSafeClient({
        apiKey: options.apiKey,
        baseURL: options.baseUrl ?? TYPESAFE_BASE_URL,
        // 요청이 작아(수 KB) 압축 이득이 거의 없고, 테스트용 가짜 서버도 단순해진다.
        compression: false,
        ...(options.fetchImpl ? { fetch: options.fetchImpl } : {}),
      })
    : null;

  return {
    provider: "typesafe",
    async decide(input: DecisionInput, model: string): Promise<Decision> {
      if (!client) {
        throw new DeciderError(
          "TYPESAFE_API_KEY가 설정되지 않았습니다.",
          "no_api_key",
        );
      }
      let result;
      try {
        result = await client.systemOne({
          model,
          state: buildState(input, options.language),
          questions,
        });
      } catch (error) {
        throw toDeciderError(error);
      }
      // SDK는 응답을 검사하지 않고 그대로 돌려주므로 저장 전에 모양을 확인한다.
      const relevance = result?.answers?.relevance;
      const impact = result?.answers?.impact;
      if (
        !isNumber(relevance?.score) ||
        !isNumber(relevance?.confidence) ||
        !(IMPACTS as readonly string[]).includes(impact?.choice) ||
        !isNumber(impact?.confidence) ||
        typeof result.model !== "string"
      ) {
        throw new DeciderError("Jev 응답 형식이 다릅니다.", "upstream");
      }
      return {
        relevance: clamp(relevance.score * RELEVANCE_SCALE, 0, 10),
        relevanceConfidence: relevance.confidence,
        impact: impact.choice as Impact,
        impactConfidence: impact.confidence,
        impactProbabilities: Object.fromEntries(
          IMPACTS.map((key) => [key, Number(impact.probabilities?.[key] ?? 0)]),
        ) as Record<Impact, number>,
        model: result.model,
        // 토큰 수를 알려 주지 않으면(null) 0으로 센다.
        usage: {
          inputTokens: result.usage?.input_tokens ?? 0,
          outputTokens: result.usage?.output_tokens ?? 0,
        },
      };
    },
  };
}

function toDeciderError(error: unknown): DeciderError {
  if (
    error instanceof AuthenticationError ||
    error instanceof PermissionDeniedError
  ) {
    return new DeciderError(
      "TypeSafe 키가 맞지 않거나 권한이 없습니다.",
      "unauthorized",
    );
  }
  if (error instanceof RateLimitError) {
    return new DeciderError("TypeSafe 호출 한도에 걸렸습니다.", "rate_limited");
  }
  if (error instanceof APITimeoutError) {
    return new DeciderError("TypeSafe 응답이 늦어 중단했습니다.", "timeout");
  }
  if (error instanceof APIError) {
    return new DeciderError(
      `TypeSafe 오류(HTTP ${error.status})`,
      error.status >= 400 && error.status < 500
        ? "invalid_request"
        : "upstream",
    );
  }
  if (error instanceof APIConnectionError) {
    return new DeciderError("TypeSafe에 연결하지 못했습니다.", "upstream");
  }
  return new DeciderError(
    error instanceof Error ? error.message : String(error),
    "upstream",
  );
}

function isNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
