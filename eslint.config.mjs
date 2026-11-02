import { defineConfig, globalIgnores } from "eslint/config";
import nextVitals from "eslint-config-next/core-web-vitals";
import nextTs from "eslint-config-next/typescript";
import prettier from "eslint-config-prettier/flat";

const eslintConfig = defineConfig([
  ...nextVitals,
  ...nextTs,
  // 포맷 규칙은 Prettier에 맡긴다(겹치는 ESLint 규칙 끔).
  prettier,
  // Override default ignores of eslint-config-next.
  globalIgnores([
    // Default ignores of eslint-config-next:
    ".next/**",
    "out/**",
    "build/**",
    "next-env.d.ts",
    // 생성물·결과물
    "dist/**",
    "test-results/**",
    "playwright-report/**",
    // 본 DB를 읽는 일회용 분석 스크립트(docker compose exec로 표준 입력에 넘기는 CJS)
    "scripts/analysis/**",
    // Rust 전환: 복사해 온 외부 파일·생성물
    "rust/**",
  ]),
]);

export default eslintConfig;
