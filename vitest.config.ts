import { defineConfig } from "vitest/config";

// 지금은 scripts/ 안의 테스트만 남았다(Rust 전환 뒤 TS 단위 테스트는 cargo test로 옮김).
export default defineConfig({
  test: {
    include: ["scripts/**/*.test.ts"],
    environment: "node",
  },
});
