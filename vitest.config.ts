import path from "node:path";
import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: {
    alias: { "@": path.resolve(import.meta.dirname, "src") },
  },
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
    // 실제 마이그레이션(drizzle/)으로 만든 DB를 테스트마다 복사해 쓴다.
    globalSetup: ["./tests/global-setup.ts"],
  },
});
