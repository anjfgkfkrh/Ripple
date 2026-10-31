// 피드 날짜 해석(`parseFeedDate`)의 TS 결과를 파일로 내보낸다(R5 완료 기준, 결정 "14단계 R5").
// 입력은 `date_samples.json`(2026-10-31 실제 피드 56개에서 받은 날짜 글자 + 경계 사례).
// Rust 테스트가 같은 입력으로 읽은 결과와 이 파일을 비교한다. 실행: pnpm exec tsx rust/tools/dump-dates.ts [출력 파일]
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { parseFeedDate } from "../../src/server/collect/rss";

const NOW = new Date("2026-10-31T09:00:00Z");
const dir = "rust/ripple/src/server/collect";
const samples = JSON.parse(
  readFileSync(path.resolve(dir, "date_samples.json"), "utf8"),
) as string[];

const dump = {
  now: NOW.toISOString(),
  cases: samples.map((raw) => ({
    raw,
    expected: parseFeedDate(raw, NOW)?.toISOString() ?? null,
  })),
};
const out = path.resolve(process.argv[2] ?? `${dir}/date_fixture.json`);
writeFileSync(out, `${JSON.stringify(dump, null, 1)}\n`);
console.log(out);
