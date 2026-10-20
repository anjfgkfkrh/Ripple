import { readAuthConfig } from "@/server/auth/config";
import { readWorkerApiToken } from "@/server/auth/worker-token";

/**
 * 비밀번호·서명 키·worker 토큰이 없거나 짧으면 web을 시작하지 않는다. (SPEC 8장)
 * 빌드 때는 비밀 값이 없으므로 검사하지 않는다.
 */
export function checkStartupConfig(): void {
  if (process.env.NEXT_PHASE === "phase-production-build") return;
  try {
    readAuthConfig();
    readWorkerApiToken();
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.error(`[설정 오류] ${message} .env를 확인하세요.`);
    process.exit(1);
  }
}
