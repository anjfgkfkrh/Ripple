// 마이그레이션 적용 진입점. Docker에서는 compose의 migrate 서비스가, 로컬에서는 `pnpm db:migrate`가 실행한다.
import path from "node:path";
import { applyMigrations } from "@/server/db/migrate";
import { requireEnv } from "@/server/env";
import { log, logError } from "@/server/log";

try {
  const databasePath = requireEnv("DATABASE_PATH");
  // 프로젝트 루트(Docker: /app)에서 실행한다고 보고 drizzle/ 폴더를 읽는다.
  applyMigrations(databasePath, path.resolve("drizzle"));
  log(`마이그레이션 적용 완료: ${databasePath}`);
} catch (error) {
  logError("마이그레이션 실패", error);
  process.exit(1);
}
