import { describe, expect, it } from "vitest";
import * as z from "zod";
import { createOpenRouterClient } from "./openrouter";
import { LlmError, type StructuredRequest } from "./types";

const schema = z.object({ answer: z.string() });
const request: StructuredRequest<{ answer: string }> = {
  model: "anthropic/claude-haiku-5.5",
  messages: [
    { role: "system", content: "시스템" },
    { role: "user", content: "질문" },
  ],
  schema,
  schemaName: "answer",
  maxTokens: 300,
};

type Captured = {
  url: string;
  init: RequestInit;
  body: Record<string, unknown>;
};

function fakeFetch(respond: () => Response | Promise<Response>) {
  const calls: Captured[] = [];
  const fetchImpl = (async (
    input: string | URL | Request,
    init?: RequestInit,
  ) => {
    calls.push({
      url: String(input),
      init: init ?? {},
      body: JSON.parse(String(init?.body ?? "{}")) as Record<string, unknown>,
    });
    return respond();
  }) as typeof fetch;
  return { fetchImpl, calls };
}

const ok = (
  content: string,
  usage = { prompt_tokens: 120, completion_tokens: 30, cost: 0.000027 },
) =>
  Response.json({
    model: "anthropic/claude-haiku-5.5",
    choices: [{ message: { content } }],
    usage,
  });

async function expectLlmError(promise: Promise<unknown>, kind: string) {
  const error = await promise.catch((e: unknown) => e);
  expect(error).toBeInstanceOf(LlmError);
  expect((error as LlmError).kind).toBe(kind);
  return error as LlmError;
}

describe("OpenRouter 클라이언트", () => {
  it("구조화 출력(strict JSON Schema)과 공급자 조건을 담아 요청한다", async () => {
    const { fetchImpl, calls } = fakeFetch(() => ok('{"answer":"네"}'));
    const client = createOpenRouterClient({ apiKey: "sk-test", fetchImpl });

    await client.generateObject(request);

    const [call] = calls;
    expect(call.url).toBe("https://openrouter.ai/api/v1/chat/completions");
    expect(new Headers(call.init.headers).get("authorization")).toBe(
      "Bearer sk-test",
    );
    expect(call.body).toMatchObject({
      model: "anthropic/claude-haiku-5.5",
      messages: request.messages,
      max_tokens: 300,
      provider: { require_parameters: true },
      response_format: {
        type: "json_schema",
        json_schema: {
          name: "answer",
          strict: true,
          schema: {
            type: "object",
            properties: { answer: { type: "string" } },
            required: ["answer"],
            additionalProperties: false,
          },
        },
      },
    });
    const sent = (
      call.body.response_format as { json_schema: { schema: object } }
    ).json_schema.schema;
    expect(sent).not.toHaveProperty("$schema");
  });

  it("응답을 검증해 돌려주고, 사용량·청구액(usage.cost)을 함께 준다", async () => {
    const { fetchImpl } = fakeFetch(() => ok('{"answer":"네"}'));
    const client = createOpenRouterClient({ apiKey: "sk-test", fetchImpl });

    const result = await client.generateObject(request);

    expect(result).toEqual({
      value: { answer: "네" },
      usage: { inputTokens: 120, outputTokens: 30, costUsd: 0.000027 },
      model: "anthropic/claude-haiku-5.5",
    });
  });

  it("코드 블록으로 감싼 JSON도 읽는다", async () => {
    const { fetchImpl } = fakeFetch(() => ok('```json\n{"answer":"네"}\n```'));
    const client = createOpenRouterClient({ apiKey: "sk-test", fetchImpl });
    expect((await client.generateObject(request)).value).toEqual({
      answer: "네",
    });
  });

  it("형식이 다르면 invalid_output이고, 청구된 사용량을 오류에 담는다", async () => {
    const { fetchImpl } = fakeFetch(() => ok('{"wrong":1}'));
    const client = createOpenRouterClient({ apiKey: "sk-test", fetchImpl });

    const error = await expectLlmError(
      client.generateObject(request),
      "invalid_output",
    );
    expect(error.usage?.costUsd).toBe(0.000027);
  });

  it.each([
    [402, "payment_required"],
    [429, "rate_limited"],
    [500, "upstream"],
  ])("HTTP %i → %s", async (status, kind) => {
    const { fetchImpl } = fakeFetch(() =>
      Response.json({ error: { message: "x", code: status } }, { status }),
    );
    const client = createOpenRouterClient({ apiKey: "sk-test", fetchImpl });
    await expectLlmError(client.generateObject(request), kind);
  });

  it("200이어도 본문에 error가 있으면 실패로 본다", async () => {
    const { fetchImpl } = fakeFetch(() =>
      Response.json({ error: { message: "provider error", code: 502 } }),
    );
    const client = createOpenRouterClient({ apiKey: "sk-test", fetchImpl });
    await expectLlmError(client.generateObject(request), "upstream");
  });

  it("제한 시간이 지나면 timeout", async () => {
    const fetchImpl = ((_: unknown, init?: RequestInit) =>
      new Promise((_resolve, reject) => {
        init?.signal?.addEventListener("abort", () =>
          reject(init.signal?.reason),
        );
      })) as typeof fetch;
    const client = createOpenRouterClient({
      apiKey: "sk-test",
      fetchImpl,
      timeoutMs: 50,
    });
    await expectLlmError(client.generateObject(request), "timeout");
  });

  it("요청에 timeoutMs가 있으면 그 요청만 그만큼 기다린다", async () => {
    const fetchImpl = ((_: unknown, init?: RequestInit) =>
      new Promise((resolve, reject) => {
        init?.signal?.addEventListener("abort", () =>
          reject(init.signal?.reason),
        );
        // 클라이언트 기본(50ms)보다 늦게, 요청의 제한 시간(500ms)보다 먼저 답한다.
        setTimeout(
          () =>
            resolve(
              Response.json({
                model: "m",
                choices: [{ message: { content: '{"answer":"늦은 답"}' } }],
                usage: { prompt_tokens: 1, completion_tokens: 1, cost: 0 },
              }),
            ),
          150,
        );
      })) as typeof fetch;
    const client = createOpenRouterClient({
      apiKey: "sk-test",
      fetchImpl,
      timeoutMs: 50,
    });
    await expectLlmError(client.generateObject(request), "timeout");
    const result = await client.generateObject({ ...request, timeoutMs: 500 });
    expect(result.value).toEqual({ answer: "늦은 답" });
  });

  it("API 키가 없으면 요청하지 않고 no_api_key", async () => {
    const { fetchImpl, calls } = fakeFetch(() => ok("{}"));
    const client = createOpenRouterClient({ apiKey: undefined, fetchImpl });
    await expectLlmError(client.generateObject(request), "no_api_key");
    expect(calls).toHaveLength(0);
  });
});
