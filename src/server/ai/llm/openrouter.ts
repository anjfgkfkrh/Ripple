import * as z from "zod";
import {
  LlmError,
  type LlmClient,
  type LlmUsage,
  type StructuredRequest,
  type StructuredResult,
} from "./types";

export const OPENROUTER_BASE_URL = "https://openrouter.ai/api/v1";

type Options = {
  apiKey: string | undefined;
  baseUrl?: string;
  timeoutMs?: number;
  fetchImpl?: typeof fetch;
};

/**
 * OpenRouter Chat Completions를 라이브러리 없이 직접 부른다.
 * - 구조화 출력(response_format: json_schema, strict)으로 JSON을 받고 zod로 한 번 더 검증한다.
 * - require_parameters: 요청의 파라미터를 모두 지원하는 공급자에게만 보낸다.
 * - 사용량·청구액은 응답의 usage에 항상 들어 있다(OpenRouter Usage Accounting).
 */
export function createOpenRouterClient({
  apiKey,
  baseUrl = OPENROUTER_BASE_URL,
  timeoutMs = 30_000,
  fetchImpl = fetch,
}: Options): LlmClient {
  return {
    provider: "openrouter",
    async generateObject<T>(
      request: StructuredRequest<T>,
    ): Promise<StructuredResult<T>> {
      if (!apiKey) {
        throw new LlmError(
          "OPENROUTER_API_KEY가 설정되지 않았습니다.",
          "no_api_key",
        );
      }

      const jsonSchema = z.toJSONSchema(request.schema) as Record<
        string,
        unknown
      >;
      // 메타 필드($schema)를 받지 않는 공급자가 있어 뺀다.
      delete jsonSchema.$schema;
      const body = {
        model: request.model,
        messages: request.messages,
        max_tokens: request.maxTokens,
        response_format: {
          type: "json_schema",
          json_schema: {
            name: request.schemaName,
            strict: true,
            schema: jsonSchema,
          },
        },
        provider: { require_parameters: true },
      };

      let response: Response;
      try {
        response = await fetchImpl(`${baseUrl}/chat/completions`, {
          method: "POST",
          headers: {
            Authorization: `Bearer ${apiKey}`,
            "Content-Type": "application/json",
          },
          body: JSON.stringify(body),
          signal: AbortSignal.timeout(timeoutMs),
        });
      } catch (error) {
        if (error instanceof Error && error.name === "TimeoutError") {
          throw new LlmError("OpenRouter 응답이 늦어 중단했습니다.", "timeout");
        }
        throw new LlmError(
          `OpenRouter에 연결하지 못했습니다: ${describe(error)}`,
          "upstream",
        );
      }

      const payload = (await response.json().catch(() => null)) as Payload;
      if (!response.ok || payload?.error) {
        throw toHttpError(response.status, payload);
      }

      const usage = readUsage(payload);
      const content = payload?.choices?.[0]?.message?.content;
      const parsed = request.schema.safeParse(parseJson(content));
      if (!parsed.success) {
        throw new LlmError(
          "OpenRouter 응답이 정해진 형식과 다릅니다.",
          "invalid_output",
          usage,
        );
      }
      return {
        value: parsed.data,
        usage,
        model: payload?.model ?? request.model,
      };
    },
  };
}

type Payload = {
  model?: string;
  choices?: { message?: { content?: string | null } }[];
  usage?: { prompt_tokens?: number; completion_tokens?: number; cost?: number };
  error?: { message?: string; code?: number };
} | null;

function readUsage(payload: Payload): LlmUsage {
  return {
    inputTokens: payload?.usage?.prompt_tokens ?? 0,
    outputTokens: payload?.usage?.completion_tokens ?? 0,
    costUsd: payload?.usage?.cost ?? 0,
  };
}

/** 대부분 순수 JSON이지만, 코드 블록으로 감싸 오는 모델에 대비한다. */
function parseJson(content: string | null | undefined): unknown {
  if (!content) return undefined;
  const text = content
    .trim()
    .replace(/^```(?:json)?\s*/i, "")
    .replace(/\s*```$/, "");
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}

function toHttpError(status: number, payload: Payload): LlmError {
  const detail = payload?.error?.message ?? `HTTP ${status}`;
  const code = payload?.error?.code ?? status;
  if (code === 402) {
    return new LlmError(
      `OpenRouter 크레딧이 부족합니다: ${detail}`,
      "payment_required",
    );
  }
  if (code === 429) {
    return new LlmError(
      `OpenRouter 요청 한도에 걸렸습니다: ${detail}`,
      "rate_limited",
    );
  }
  return new LlmError(`OpenRouter 오류: ${detail}`, "upstream");
}

function describe(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
