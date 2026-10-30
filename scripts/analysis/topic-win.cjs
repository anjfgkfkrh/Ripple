const Database = require("better-sqlite3");
const db = new Database("/data/ripple.db", {
  readonly: true,
  fileMustExist: true,
});
const H = 3600e3;
const FH = db
  .prepare(
    "select title,published_at p from feed_headlines order by published_at desc, id desc",
  )
  .all();
const minP = Math.min(...FH.map((h) => h.p));
const ja = (t) => /[぀-ヿ]/.test(t);
// refresh starts = topic created_at clusters + usage log times since full 24h coverage
const times = db
  .prepare(
    "select at from usage_logs where purpose='topic_refresh' and at >= ? order by at",
  )
  .all(minP + 24 * H)
  .map((r) => r.at);
console.log(
  "시각 | 지금 방식 제목 수 | 24시간 제목 수(일본어) | 최근 1500개가 덮는 시간",
);
let prev = null;
const prevAt = db.prepare(
  "select max(at) a from usage_logs where purpose='topic_refresh' and at < ? and input_tokens>0",
);
for (const t of times) {
  const last = prevAt.get(t).a;
  const cur = FH.filter((h) => h.p >= last && h.p <= t).length;
  const day = FH.filter((h) => h.p >= t - 24 * H && h.p <= t);
  const top = day.slice(0, 1500);
  const cover = top.length ? (t - top[top.length - 1].p) / H : 0;
  console.log(
    `${new Date(t + 9 * H).toISOString().slice(5, 16)} | ${cur} | ${day.length}(${day.filter((h) => ja(h.title)).length}) | ${cover.toFixed(1)}h`,
  );
}
