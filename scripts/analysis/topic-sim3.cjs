const fs = require("fs");
const Database = require("better-sqlite3");
const db = new Database("/data/ripple.db", {
  readonly: true,
  fileMustExist: true,
});
const lines = fs.readFileSync(0, "utf8").trim().split("\n");
const R = lines
  .map((l) => {
    const m = l.match(
      /^\[(.+?)\] 토픽 갱신: 제목 (\d+)개 → 토픽 (\d+)개\(이어짐 (\d+), 새로 (\d+), 끝남 (\d+)\).*, ([\d.]+)초$/,
    );
    if (!m) return null;
    const end = Date.parse(m[1].replace(" ", "T") + "+09:00");
    return {
      end,
      start: end - Number(m[7]) * 1000 - 2000,
      titles: +m[2],
      topics: +m[3],
      cont: +m[4],
      nw: +m[5],
      ended: +m[6],
    };
  })
  .filter(Boolean);
const P = new Map(
  db
    .prepare(
      `select s.topic_id tid, sum(e.relevance>=6 and e.magnitude>=6) pass from evaluations e join stocks s on s.id=e.stock_id where s.kind='topic' group by 1`,
    )
    .all()
    .map((r) => [r.tid, r.pass]),
);
const T = db
  .prepare("select id,created_at c from topics where source='auto'")
  .all();
const b = (t) =>
  t < 200
    ? "a 200개 미만"
    : t < 500
      ? "b 200~499"
      : t < 1000
        ? "c 500~999"
        : "d 1000개 이상";
const agg = {};
for (const r of R) {
  const k = b(r.titles);
  const a = (agg[k] ??= { n: 0, ended: 0, nw: 0, topics: 0, newT: 0, zero: 0 });
  a.n++;
  a.ended += r.ended;
  a.nw += r.nw;
  a.topics += r.topics;
  const made = T.filter((t) => t.c >= r.start && t.c <= r.end);
  a.newT += made.length;
  a.zero += made.filter((t) => !(P.get(t.id) > 0)).length;
}
console.log(
  "제목 수 | 갱신 횟수 | 평균 끝남 | 평균 새로 | 평균 토픽 수 | 새 토픽 중 기준 이상 기사 0건",
);
for (const k of Object.keys(agg).sort()) {
  const a = agg[k];
  console.log(
    `${k} | ${a.n} | ${(a.ended / a.n).toFixed(1)} | ${(a.nw / a.n).toFixed(1)} | ${(a.topics / a.n).toFixed(1)} | ${a.zero}/${a.newT}`,
  );
}
console.log(
  "토픽 수 10 미만으로 낸 갱신:",
  R.filter((r) => r.topics < 10)
    .map(
      (r) =>
        `${new Date(r.end + 9 * 3600e3).toISOString().slice(5, 16)} 제목${r.titles} 토픽${r.topics}`,
    )
    .join(", "),
);
