type HeaderReader = { get(name: string): string | null };

/**
 * 로그인 실패 제한에 쓰는 접속자 키. Cloudflare Tunnel을 거친 요청에는 Cloudflare가 접속자 IP를
 * CF-Connecting-IP에 넣는다(클라이언트가 보낸 값은 덮어씀). 이 헤더가 없으면 터널을 거치지 않은
 * 로컬 접속이므로 하나로 묶는다. 위조하기 쉬운 X-Forwarded-For는 쓰지 않는다.
 */
export function clientKey(headers: HeaderReader): string {
  const ip = headers.get("cf-connecting-ip")?.trim();
  return ip ? `ip:${ip}` : "direct";
}

/** HTTPS로 들어온 요청이면 세션 쿠키에 Secure를 붙인다(터널은 HTTPS, 로컬 접속은 HTTP). */
export function isHttpsRequest(headers: HeaderReader): boolean {
  return headers.get("x-forwarded-proto")?.split(",")[0]?.trim() === "https";
}

const BASE = "http://ripple.local";

/** 로그인 후 이동할 경로. 같은 사이트 안의 경로만 허용해 외부 주소로 보내는 공격을 막는다. */
export function safeNextPath(value: unknown): string {
  if (typeof value !== "string" || !value.startsWith("/")) return "/";
  let url: URL;
  try {
    url = new URL(value, BASE);
  } catch {
    return "/";
  }
  if (url.origin !== BASE || url.pathname === "/login") return "/";
  return `${url.pathname}${url.search}${url.hash}`;
}
