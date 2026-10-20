import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import type { TestProject } from "vitest/node";
import { applyMigrations } from "../src/server/db/migrate";

declare module "vitest" {
  export interface ProvidedContext {
    templateDbPath: string;
  }
}

/** 실제 마이그레이션(drizzle/)으로 템플릿 DB를 한 번 만든다. 테스트는 이 파일을 복사해 쓴다. */
export default function setup(project: TestProject) {
  const dir = mkdtempSync(path.join(tmpdir(), "ripple-test-"));
  const templateDbPath = path.join(dir, "template.db");
  applyMigrations(templateDbPath, path.resolve("drizzle"));
  project.provide("templateDbPath", templateDbPath);
  return () => rmSync(dir, { recursive: true, force: true });
}
