// OpenRouter에 보내는 구조화 출력 스키마(zod → JSON Schema)를 파일로 내보낸다(docs/RUST_MIGRATION.md 3.11).
// Rust 테스트가 Rust가 만든 스키마와 이 파일을 비교한다. 실행: pnpm exec tsx rust/tools/dump-schemas.ts
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import * as z from "zod";
import { keywordTreeSchema } from "../../src/server/keywords/tree";
import { matchOutputSchema } from "../../src/server/stocks/match";
import { topicRefreshSchema } from "../../src/server/topics/refresh";

const SCHEMAS = {
  stock_candidates: matchOutputSchema,
  keyword_tree: keywordTreeSchema,
  topic_refresh: topicRefreshSchema,
};

// 인자로 폴더를 주면 거기에 쓴다(Rust 테스트가 파일이 최신인지 확인할 때).
const outDir = path.resolve(
  process.argv[2] ?? "rust/ripple/src/server/ai/llm/schemas",
);
mkdirSync(outDir, { recursive: true });
for (const [name, schema] of Object.entries(SCHEMAS)) {
  // openrouter.ts와 같은 처리: $schema를 뺀다.
  const jsonSchema = z.toJSONSchema(schema) as Record<string, unknown>;
  delete jsonSchema.$schema;
  writeFileSync(
    path.join(outDir, `${name}.json`),
    `${JSON.stringify(jsonSchema, null, 2)}\n`,
  );
  console.log(`${name}.json`);
}
