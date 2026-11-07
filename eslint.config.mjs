import { defineConfig, globalIgnores } from "eslint/config";
import tseslint from "typescript-eslint";
import prettier from "eslint-config-prettier/flat";

const eslintConfig = defineConfig([
  tseslint.configs.recommended,
  // 포맷 규칙은 Prettier에 맡긴다(겹치는 ESLint 규칙 끔).
  prettier,
  globalIgnores([
    // 생성물·결과물
    "dist/**",
    "test-results/**",
    "playwright-report/**",
    ".e2e/**",
    "release-dist/**",
    // Rust 전환: Rust 코드와 그 도구 폴더
    "rust/**",
  ]),
]);

export default eslintConfig;
