const Database = require("better-sqlite3");
const db = new Database("/data/ripple.db", {
  readonly: true,
  fileMustExist: true,
});
const H = 3600e3,
  now = Date.now(),
  since = now - 24 * H;
const norm = (s) => s.toLowerCase().replace(/\s+/g, " ");
const words = (s) =>
  norm(s)
    .split(" ")
    .map((w) => w.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, ""))
    .filter(Boolean);
const AW = /^[a-z0-9][a-z0-9.&+'-]*$/;
const has = (h, w) =>
  AW.test(w)
    ? new RegExp(
        `(?<![a-z0-9])${w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?![a-z0-9])`,
      ).test(h)
    : h.includes(w);
const X = db
  .prepare("select title,summary from feed_headlines where published_at>=?")
  .all(since)
  .map((h) => norm(h.title + " " + (h.summary || "")));
const lensCnt = db.prepare(
  `select count(distinct m.article_id) n, count(distinct case when e.relevance>=6 and e.magnitude>=6 then m.article_id end) shown from article_matches m join articles a on a.id=m.article_id join stocks s on s.id=m.stock_id left join evaluations e on e.match_id=m.id where s.topic_id=? and s.market='KR' and a.published_at>=?`,
);
for (const t of db
  .prepare(
    "select id,name,terms from topics where source='auto' and status='active'",
  )
  .all()) {
  const ws = JSON.parse(t.terms).map(words);
  const fh = X.filter((h) =>
    ws.some((w) => w.length && w.every((x) => has(h, x))),
  ).length;
  const r = lensCnt.get(t.id, since);
  console.log(`${fh}\t${r.n}\t${r.shown}\t${t.name}`);
}
