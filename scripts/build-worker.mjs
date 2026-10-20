// worker와 마이그레이션 진입점을 각각 파일 하나로 묶는다.
// Docker worker 이미지에는 이 번들, 마이그레이션 SQL(drizzle/), 네이티브 모듈(better-sqlite3)만 들어간다.
import { build } from "esbuild";

await build({
  entryPoints: ["src/worker/index.ts", "src/worker/migrate.ts"],
  outdir: "dist/worker",
  outExtension: { ".js": ".mjs" },
  bundle: true,
  platform: "node",
  target: "node24",
  format: "esm",
  sourcemap: true,
  // 네이티브 바이너리(.node)는 묶을 수 없으므로 node_modules에서 읽는다.
  external: ["better-sqlite3"],
  // 번들된 CommonJS 코드가 require를 쓸 수 있게 한다.
  banner: {
    js: 'import { createRequire } from "node:module"; const require = createRequire(import.meta.url);',
  },
  logLevel: "info",
});
