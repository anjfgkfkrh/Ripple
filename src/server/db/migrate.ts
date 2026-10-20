import { mkdirSync } from "node:fs";
import path from "node:path";
import Database from "better-sqlite3";
import { drizzle } from "drizzle-orm/better-sqlite3";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";

/**
 * 아직 적용하지 않은 마이그레이션을 적용한다. DB 파일이 없으면 만든다(DB 파일을 만드는 유일한 곳).
 * 배타 잠금을 걸지 않으므로 web·worker가 DB를 열고 있어도 적용된다.
 */
export function applyMigrations(
  databasePath: string,
  migrationsFolder: string,
): void {
  const file = path.resolve(databasePath);
  mkdirSync(path.dirname(file), { recursive: true });
  const sqlite = new Database(file, { timeout: 5000 });
  try {
    migrate(drizzle(sqlite), { migrationsFolder });
  } finally {
    sqlite.close();
  }
}
