// TS가 만드는 LLM 프롬프트를 파일로 내보낸다(docs/RUST_MIGRATION.md 6장: 프롬프트 글자 비교 테스트).
// Rust 테스트가 같은 입력으로 만든 프롬프트와 이 파일을 글자로 비교한다. 하위 단계(R4·R7)에서 항목을 더한다.
// 실행: pnpm exec tsx rust/tools/dump-prompts.ts [출력 폴더]
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { buildMatchMessages } from "../../src/server/stocks/match";

const outDir = path.resolve(
  process.argv[2] ?? "rust/ripple/src/server/prompts",
);
mkdirSync(outDir, { recursive: true });

const FILES: Record<string, unknown> = {
  "match.json": ["삼전", "NVDA", "  공백 있는 입력  "].map((query) => ({
    query,
    messages: buildMatchMessages(query),
  })),
};
for (const [name, value] of Object.entries(FILES)) {
  writeFileSync(path.join(outDir, name), `${JSON.stringify(value, null, 2)}\n`);
  console.log(name);
}
