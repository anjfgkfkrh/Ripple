/**
 * E2E 전용 값(실제 비밀이 아님). 로컬 E2E는 이 값으로 web·worker를 띄우고,
 * Docker를 검사할 때는 같은 값을 환경변수로 넘겨 띄운다(README 참고).
 */
export const E2E_APP_PASSWORD =
  process.env.E2E_APP_PASSWORD ?? "e2e-password-not-secret";
export const E2E_SESSION_SECRET =
  process.env.E2E_SESSION_SECRET ?? "e2e-session-secret-not-for-production-use";
export const E2E_WORKER_API_TOKEN = "e2e-worker-token-not-for-production-use";

export const E2E_PORTS = { web: 3100, worker: 4100, mock: 4200 } as const;
/** 테스트용 worker·web이 쓰는 DB 파일(실행할 때마다 새로 만든다) */
export const E2E_DB_PATH = ".e2e/ripple.db";
