/**
 * 수집용 HTTP 클라이언트. 같은 사이트에는 요청 시작 간격을 둔다(CLAUDE.md: 크롤링은 요청 간격을 둔다).
 * User-Agent는 개인용 RSS 리더임을 그대로 밝힌다.
 */
export const COLLECTOR_USER_AGENT =
  "Mozilla/5.0 (compatible; Ripple/0.1; personal RSS reader)";

export type HttpResponse = {
  status: number;
  text: string;
  contentType: string;
};

export type HttpClient = {
  get(url: string, headers?: Record<string, string>): Promise<HttpResponse>;
};

export class HttpError extends Error {
  constructor(
    message: string,
    readonly kind: "timeout" | "network" | "too_large",
  ) {
    super(message);
    this.name = "HttpError";
  }
}

export function createHttpClient(
  options: {
    userAgent?: string;
    /** 같은 호스트에 대한 요청 시작 간격 */
    minIntervalMs?: number;
    timeoutMs?: number;
    maxBytes?: number;
    fetchImpl?: typeof fetch;
    sleep?: (ms: number) => Promise<void>;
  } = {},
): HttpClient {
  const {
    userAgent = COLLECTOR_USER_AGENT,
    minIntervalMs = 1100,
    timeoutMs = 20_000,
    maxBytes = 5_000_000,
    fetchImpl = fetch,
    sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
  } = options;
  // 호스트마다 다음 요청을 시작해도 되는 시각. 자리를 먼저 잡아 두므로 동시에 불러도 간격이 지켜진다.
  const nextAt = new Map<string, number>();

  async function waitTurn(host: string): Promise<void> {
    const now = Date.now();
    const at = Math.max(now, nextAt.get(host) ?? 0);
    nextAt.set(host, at + minIntervalMs);
    if (at > now) await sleep(at - now);
  }

  return {
    async get(url, headers = {}) {
      await waitTurn(new URL(url).host);
      let response: Response;
      try {
        response = await fetchImpl(url, {
          headers: {
            "User-Agent": userAgent,
            Accept:
              "application/rss+xml, application/atom+xml, application/xml;q=0.9, application/json;q=0.9, */*;q=0.5",
            ...headers,
          },
          redirect: "follow",
          signal: AbortSignal.timeout(timeoutMs),
        });
      } catch (error) {
        if (error instanceof Error && error.name === "TimeoutError") {
          throw new HttpError("응답이 늦어 중단했습니다.", "timeout");
        }
        throw new HttpError(
          `연결하지 못했습니다: ${error instanceof Error ? error.message : String(error)}`,
          "network",
        );
      }
      const bytes = await readLimited(response, maxBytes);
      const contentType = response.headers.get("content-type") ?? "";
      return {
        status: response.status,
        text: decode(bytes, contentType),
        contentType,
      };
    },
  };
}

async function readLimited(
  response: Response,
  maxBytes: number,
): Promise<Uint8Array> {
  if (!response.body) return new Uint8Array();
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > maxBytes) {
      await reader.cancel();
      throw new HttpError(
        `응답이 너무 큽니다(${maxBytes}바이트 초과).`,
        "too_large",
      );
    }
    chunks.push(value);
  }
  const out = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    out.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return out;
}

/** Content-Type이나 XML 선언의 인코딩으로 읽는다(EUC-KR 피드 대비). 모르면 UTF-8. */
export function decode(bytes: Uint8Array, contentType: string): string {
  const head = new TextDecoder("latin1").decode(bytes.subarray(0, 300));
  const label =
    contentType.match(/charset=["']?([\w-]+)/i)?.[1] ??
    head.match(/<\?xml[^>]*encoding=["']([\w-]+)["']/i)?.[1] ??
    "utf-8";
  const normalized = /^(cp949|ms949|x-windows-949)$/i.test(label)
    ? "euc-kr"
    : label;
  try {
    return new TextDecoder(normalized).decode(bytes);
  } catch {
    return new TextDecoder("utf-8").decode(bytes);
  }
}
