type Env = Record<string, string | undefined>;

/** 값이 없으면 바로 실패한다. 비밀 값이 들어올 수 있으므로 값은 메시지에 넣지 않는다. */
export function requireEnv(name: string, env: Env = process.env): string {
  const value = env[name]?.trim();
  if (!value) {
    throw new Error(`환경변수 ${name}이(가) 설정되지 않았습니다.`);
  }
  return value;
}

export function readPort(
  name: string,
  fallback: number,
  env: Env = process.env,
): number {
  const raw = env[name]?.trim();
  if (!raw) return fallback;
  const port = Number(raw);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error(`환경변수 ${name}의 값이 올바른 포트 번호가 아닙니다.`);
  }
  return port;
}
