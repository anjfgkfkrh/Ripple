import { OPENROUTER_BASE_URL } from "./openrouter";

/** 모델 목록을 확인하지 못했을 때(네트워크·응답 형식) */
export class ModelListError extends Error {}

const MAX_PAGES = 10;

/**
 * OpenRouter에 이 모델 ID가 있는지 공개 모델 목록(GET /models, 키 불필요·무료)으로 확인한다
 * (설정 화면 저장 때, 2026-10-21 결정). 2026-10-21 실측: { data: [{ id, … }], links: { next } }.
 */
export async function isOpenRouterModel(
  modelId: string,
  options: {
    baseUrl?: string;
    fetchImpl?: typeof fetch;
    timeoutMs?: number;
  } = {},
): Promise<boolean> {
  const fetchImpl = options.fetchImpl ?? fetch;
  let url: string | null = `${options.baseUrl ?? OPENROUTER_BASE_URL}/models`;
  for (let page = 0; url && page < MAX_PAGES; page += 1) {
    let body: { data?: { id?: unknown }[]; links?: { next?: unknown } };
    try {
      const response = await fetchImpl(url, {
        signal: AbortSignal.timeout(options.timeoutMs ?? 15_000),
      });
      if (!response.ok) {
        throw new ModelListError(
          `OpenRouter 모델 목록 HTTP ${response.status}`,
        );
      }
      body = (await response.json()) as typeof body;
    } catch (error) {
      if (error instanceof ModelListError) throw error;
      throw new ModelListError(
        `OpenRouter 모델 목록을 받지 못했습니다: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
    if (!Array.isArray(body.data)) {
      throw new ModelListError("OpenRouter 모델 목록 형식이 다릅니다.");
    }
    if (body.data.some((model) => model.id === modelId)) return true;
    url = typeof body.links?.next === "string" ? body.links.next : null;
  }
  return false;
}
