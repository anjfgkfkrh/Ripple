/** 외부 접속 임시 주소 상태(9단계). off: 터널을 쓰지 않음(로컬 실행), pending: 주소를 받는 중 */
export type TunnelStatus =
  | { state: "off" }
  | { state: "pending" }
  | { state: "ready"; url: string }
  | { state: "error"; reason: string };

const HOSTNAME = /^[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+$/;

/**
 * cloudflared Quick Tunnel의 지금 주소를 읽는다. cloudflared 메트릭 서버의 `/quicktunnel`은
 * `{"hostname":"…"}`를 준다(2026-10-21 cloudflared 소스로 확인). 주소는 재시작마다 바뀐다.
 */
export async function fetchTunnelStatus(
  metricsUrl: string | undefined,
  options: { timeoutMs?: number; fetchImpl?: typeof fetch } = {},
): Promise<TunnelStatus> {
  if (!metricsUrl?.trim()) return { state: "off" };
  const fetchImpl = options.fetchImpl ?? fetch;
  try {
    const response = await fetchImpl(new URL("/quicktunnel", metricsUrl), {
      signal: AbortSignal.timeout(options.timeoutMs ?? 1500),
      cache: "no-store",
    });
    if (!response.ok)
      return { state: "error", reason: `HTTP ${response.status}` };
    const body = (await response.json()) as { hostname?: unknown };
    const hostname =
      typeof body.hostname === "string" ? body.hostname.trim() : "";
    if (!hostname) return { state: "pending" };
    if (!HOSTNAME.test(hostname)) {
      return { state: "error", reason: "주소 형식이 다릅니다." };
    }
    return { state: "ready", url: `https://${hostname}` };
  } catch (error) {
    const cause =
      error instanceof Error && error.cause instanceof Error
        ? ` (${error.cause.message})`
        : "";
    return {
      state: "error",
      reason: `${error instanceof Error ? error.message : String(error)}${cause}`,
    };
  }
}
