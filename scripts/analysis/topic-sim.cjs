const Database = require("better-sqlite3");
const db = new Database("/data/ripple.db", {
  readonly: true,
  fileMustExist: true,
});
const H = 3600e3,
  START = Date.parse("2026-10-23T00:00:00+09:00");
const key = (s) => s.replace(/\s+/g, "").toLowerCase();
const T = db
  .prepare(
    "select id,name,terms,created_at c,ended_at e,status from topics where source='auto' order by created_at,id",
  )
  .all()
  .map((t) => ({ ...t, terms: JSON.parse(t.terms) }));
const NOW = Date.now();
const same = (a, b) =>
  key(a.name) === key(b.name) ||
  a.terms.some((x) => b.terms.map(key).includes(key(x)));
// passes / evaluations per topic (all lenses)
const ev = db
  .prepare(
    `select s.topic_id tid, count(*) n, sum(e.copied_from is null) calls, sum(e.relevance>=6 and e.magnitude>=6) pass,
 sum(case when abs(m.created_at - t.created_at) < 300000 and a.published_at < t.created_at then 1 else 0 end) bf,
 sum(case when abs(m.created_at - t.created_at) < 300000 and a.published_at < t.created_at and e.copied_from is null then 1 else 0 end) bfcalls
 from evaluations e join article_matches m on m.id=e.match_id join articles a on a.id=e.article_id join stocks s on s.id=e.stock_id join topics t on t.id=s.topic_id
 where s.kind='topic' and t.source='auto' group by 1`,
  )
  .all();
const E = new Map(ev.map((r) => [r.tid, r]));
const inRange = T.filter((t) => t.c >= START);
// 1) re-creations absorbed: new topic N whose same-subject predecessor P ended before N but P.c+24h > N.c
let absorbedMin = [],
  absorbedRevive = [];
for (const n of inRange) {
  const prevs = T.filter(
    (p) => p.id !== n.id && p.c < n.c && p.e && p.e <= n.c && same(p, n),
  );
  if (prevs.some((p) => p.c + 24 * H > n.c)) absorbedMin.push(n);
  if (prevs.some((p) => p.e + 24 * H > n.c)) absorbedRevive.push(n);
}
const sumBf = (arr) => arr.reduce((s, n) => s + (E.get(n.id)?.bfcalls || 0), 0);
console.log(
  "topics since 10-23:",
  inRange.length,
  "ended:",
  inRange.filter((t) => t.e).length,
);
console.log(
  "absorbed (min24h, name/term rule):",
  absorbedMin.length,
  "backfill calls saved",
  sumBf(absorbedMin),
);
console.log(
  "absorbed (revive within 24h after end, name/term rule):",
  absorbedRevive.length,
  "backfill calls saved",
  sumBf(absorbedRevive),
);
console.log(
  "total backfill calls all topics:",
  inRange.reduce((s, n) => s + (E.get(n.id)?.bfcalls || 0), 0),
  "total calls:",
  inRange.reduce((s, n) => s + (E.get(n.id)?.calls || 0), 0),
);
// 2) short-lived topics usefulness
const life = (t) => ((t.e ?? NOW) - t.c) / H;
const short = inRange.filter((t) => t.e && life(t) < 12);
const shortZero = short.filter((t) => (E.get(t.id)?.pass || 0) === 0);
console.log(
  "ended <12h:",
  short.length,
  "of which 0 passing articles:",
  shortZero.length,
);
console.log(
  "  samples 0-pass:",
  shortZero
    .slice(0, 40)
    .map((t) => t.name)
    .join(" | "),
);
const all = inRange.filter((t) => t.e);
console.log(
  "pass per topic: <12h avg",
  (
    short.reduce((s, t) => s + (E.get(t.id)?.pass || 0), 0) / short.length
  ).toFixed(2),
  " >=12h avg",
  (
    all
      .filter((t) => life(t) >= 12)
      .reduce((s, t) => s + (E.get(t.id)?.pass || 0), 0) /
    all.filter((t) => life(t) >= 12).length
  ).toFixed(2),
);
// 3) concurrency at refresh times
const times = [
  ...new Set(T.filter((t) => t.c >= START).map((t) => t.c)),
].sort();
const absorbedIds = new Set(absorbedMin.map((t) => t.id));
let act = [],
  rule = [];
for (const t of times) {
  act.push(T.filter((x) => x.c <= t && (!x.e || x.e > t)).length);
  rule.push(
    T.filter(
      (x) =>
        !absorbedIds.has(x.id) &&
        x.c <= t &&
        (!x.e || Math.max(x.e, x.c + 24 * H) > t),
    ).length,
  );
}
const st = (a) =>
  `avg ${(a.reduce((s, x) => s + x, 0) / a.length).toFixed(1)} max ${Math.max(...a)}`;
console.log(
  "active topics at refresh: actual",
  st(act),
  "| with min 24h",
  st(rule),
);
// 4) extra matches if kept alive: use feed_headlines (covers since 10-27 12:32)
const FH = db
  .prepare(
    "select title, summary, published_at p, coalesce(url_key,title_key) k from feed_headlines",
  )
  .all();
const minFH = Math.min(...FH.map((h) => h.p));
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
const H2 = FH.map((h) => ({
  p: h.p,
  k: h.k,
  t: norm(h.title + " " + (h.summary || "")),
}));
let extraM = 0,
  nTop = 0,
  savedBf = 0;
for (const p of inRange) {
  if (!p.e || p.e < minFH || p.c + 24 * H <= p.e) continue;
  const later = inRange.find(
    (n) =>
      n.c > p.e && n.c < p.c + 24 * H && same(p, n) && absorbedIds.has(n.id),
  );
  const end = Math.min(p.c + 24 * H, later ? later.c : Infinity, NOW);
  const ws = p.terms.map(words);
  const seen = new Set();
  for (const h of H2) {
    if (h.p < p.e || h.p >= end || seen.has(h.k)) continue;
    if (ws.some((w) => w.length && w.every((x) => has(h.t, x)))) seen.add(h.k);
  }
  extraM += seen.size;
  nTop++;
}
const savedRecent = absorbedMin.filter((n) => n.c >= minFH);
console.log(
  `since ${new Date(minFH).toISOString()}: topics kept longer ${nTop}, extra matched headlines per lens ${extraM}; absorbed re-creations ${savedRecent.length}, their backfill calls ${sumBf(savedRecent)}`,
);
// list absorbed pairs
for (const n of absorbedMin) {
  const p = T.filter(
    (p) => p.c < n.c && p.e && p.e <= n.c && same(p, n) && p.c + 24 * H > n.c,
  ).pop();
  console.log(
    `  ${p.id} ${p.name} -> ${n.id} ${n.name} (+${((n.c - p.e) / H).toFixed(1)}h after end)`,
  );
}
