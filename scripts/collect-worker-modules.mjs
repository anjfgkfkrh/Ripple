// worker 이미지에 넣을 네이티브 모듈(better-sqlite3)만 <출력 폴더>/node_modules로 복사한다.
// 나머지 의존성은 esbuild 번들(dist/worker)에 들어 있다. 찾지 못하면 예외로 빌드를 멈춘다.
// better-sqlite3 13은 실행할 때 prebuilds/의 바이너리만 읽고 다른 JS 의존성이 없다.
import { cpSync, rmSync } from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";

const outDir = process.argv[2];
if (!outDir) {
  throw new Error(
    "사용법: node scripts/collect-worker-modules.mjs <출력 폴더>",
  );
}

const packageJson = createRequire(path.resolve("package.json")).resolve(
  "better-sqlite3/package.json",
);
const target = path.join(outDir, "node_modules", "better-sqlite3");
cpSync(path.dirname(packageJson), target, {
  recursive: true,
  dereference: true,
});
console.log(`복사: better-sqlite3 ← ${path.dirname(packageJson)}`);

// SQLite 원본 소스와 C++ 소스는 소스 빌드용이라 실행에 필요 없다.
for (const sub of ["deps", "src"]) {
  rmSync(path.join(target, sub), { recursive: true, force: true });
}
