/** 추적용으로만 붙는 쿼리 파라미터(같은 기사를 다른 기사로 보지 않게 뺀다) */
const TRACKING_PARAM = /^(utm_[a-z]+|fbclid|gclid|plink|cooper)$/i;

/**
 * 같은 기사인지 가리는 키(D-19). http/https 차이, 끝 슬래시, 조각(#), 추적 파라미터를 무시한다.
 * http(s) 주소가 아니면 null.
 */
export function urlKey(raw: string): string | null {
  let url: URL;
  try {
    url = new URL(raw.trim());
  } catch {
    return null;
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") return null;
  const params = [...url.searchParams.entries()]
    .filter(([key]) => !TRACKING_PARAM.test(key))
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  const query = params.length > 0 ? `?${new URLSearchParams(params)}` : "";
  const path = url.pathname.replace(/\/+$/, "") || "/";
  const port = url.port ? `:${url.port}` : "";
  return `${url.hostname.toLowerCase()}${port}${path}${query}`;
}
