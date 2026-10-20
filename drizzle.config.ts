import { defineConfig } from "drizzle-kit";

// `pnpm db:generate --name <이름>`: 스키마 변경을 비교해 drizzle/에 SQL 마이그레이션을 만든다(DB 연결 불필요).
export default defineConfig({
  dialect: "sqlite",
  schema: "./src/server/db/schema.ts",
  out: "./drizzle",
});
