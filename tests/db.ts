import { copyFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { inject } from "vitest";

/** 마이그레이션이 끝난 템플릿 DB를 복사해 테스트 전용 DB 파일을 만든다. */
export function createTestDbFile(): {
  path: string;
  dir: string;
  cleanup: () => void;
} {
  const dir = mkdtempSync(path.join(tmpdir(), "ripple-db-"));
  const dbPath = path.join(dir, "ripple.db");
  copyFileSync(inject("templateDbPath"), dbPath);
  return {
    path: dbPath,
    dir,
    cleanup: () => rmSync(dir, { recursive: true, force: true }),
  };
}
