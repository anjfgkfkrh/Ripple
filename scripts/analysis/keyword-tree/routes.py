"""키워드 트리 유용성 측정. 사용: python3 -I routes.py <ripple.db> <시작 KST 'YYYY-MM-DD HH:MM'> [끝]"""
import sqlite3
import sys
from collections import Counter, defaultdict
from datetime import datetime, timedelta, timezone

KST = timezone(timedelta(hours=9))
DB = sys.argv[1]


def ms(s):
    return int(datetime.strptime(s, "%Y-%m-%d %H:%M").replace(tzinfo=KST).timestamp() * 1000)


START = ms(sys.argv[2])
END = ms(sys.argv[3]) if len(sys.argv) > 3 else 10**15
MIN_REL, MIN_MAG = 6.0, 5.0

con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
con.row_factory = sqlite3.Row

stocks = {r["id"]: r for r in con.execute("select * from stocks")}
kw_kind = {}
for r in con.execute("select stock_id, path, kind from keywords"):
    kw_kind[(r["stock_id"], r["path"])] = r["kind"]

rows = con.execute(
    """
    select m.id mid, m.article_id aid, m.stock_id sid, m.keyword_path path, m.match_kind mk, m.level lv, m.gated g,
           e.relevance rel, e.magnitude mag, e.copied_from cp, a.title, a.summary, a.published_at pub
    from article_matches m
    join articles a on a.id = m.article_id
    left join evaluations e on e.match_id = m.id
    where m.created_at >= ? and m.created_at < ?
    """,
    (START, END),
).fetchall()


def route(r):
    s = stocks[r["sid"]]
    kind = s["kind"]
    if kind == "topic":
        return "auto-topic"
    if r["mk"] in ("name", "direct"):
        return f"{kind}:{r['mk']}"
    k = kw_kind.get((r["sid"], r["path"]))
    if k == "current":
        sub = "topic-branch"
    elif k == "structure":
        sub = "structure"
    else:
        sub = "old-path"
    return f"{kind}:kw{r['lv']}:{sub}"


def passed(r):
    if r["rel"] is None:
        return False
    return r["rel"] >= MIN_REL and (r["mag"] is None or r["mag"] >= MIN_MAG)


print(f"# 창 {sys.argv[2]} ~ {sys.argv[3] if len(sys.argv) > 3 else 'end'}  연결 {len(rows)}")
print("\n## 경로별: 연결 / gated / 평가(복사 제외) / 복사 / 미평가 / 통과")
agg = defaultdict(Counter)
for r in rows:
    k = route(r)
    c = agg[k]
    c["match"] += 1
    if r["g"]:
        c["gated"] += 1
    elif r["rel"] is None:
        c["unevaluated"] += 1
    elif r["cp"] is not None:
        c["copied"] += 1
    else:
        c["evaluated"] += 1
    if passed(r):
        c["passed"] += 1
for k in sorted(agg):
    c = agg[k]
    print(f"{k:32s} {c['match']:6d} {c['gated']:6d} {c['evaluated']:6d} {c['copied']:5d} {c['unevaluated']:5d} {c['passed']:5d}")

# 기사 단위: 통과한 경로 집합
by_article = defaultdict(set)
by_article_all = defaultdict(set)
for r in rows:
    k = route(r)
    by_article_all[r["aid"]].add(k)
    if passed(r):
        by_article[r["aid"]].add((k, r["sid"]))


def is_kw(k):
    return ":kw" in k


print("\n## 통과 기사 단위 고유 기여")
tot = len(by_article)
kw_only = [a for a, s in by_article.items() if s and all(is_kw(k) for k, _ in s)]
kw_any = [a for a, s in by_article.items() if any(is_kw(k) for k, _ in s)]
print(f"통과 기사 {tot}, 키워드 경로 통과 포함 {len(kw_any)}, 키워드 경로로만 통과 {len(kw_only)}")
for kind in ("stock", "market"):
    only = [a for a, s in by_article.items() if s and all(k.startswith(kind + ":kw") for k, _ in s)]
    anyk = [a for a, s in by_article.items() if any(k.startswith(kind + ":kw") for k, _ in s)]
    print(f"  {kind}: 키워드 통과 포함 {len(anyk)}, 그 경로로만 {len(only)}")

# 종목별 통과(경로별)
print("\n## 항목별 통과(이름·별칭 vs 키워드)")
per = defaultdict(Counter)
for r in rows:
    if passed(r) and stocks[r["sid"]]["kind"] != "topic":
        per[stocks[r["sid"]]["name_ko"] or stocks[r["sid"]]["name"]][route(r)] += 1
for n, c in per.items():
    print(n, dict(c))

# 키워드별(구조·토픽 가지)
print("\n## 키워드별: 연결 / 평가 / 통과 (종목·시장)")
kwc = defaultdict(Counter)
for r in rows:
    if is_kw(route(r)):
        key = (stocks[r["sid"]]["name_ko"] or stocks[r["sid"]]["name"], r["path"], route(r).split(":")[-1])
        kwc[key]["m"] += 1
        if not r["g"] and r["rel"] is not None and r["cp"] is None:
            kwc[key]["e"] += 1
        if passed(r):
            kwc[key]["p"] += 1
for key, c in sorted(kwc.items(), key=lambda x: (x[0][0], -x[1]["p"], -x[1]["m"])):
    print(f"{c['m']:5d} {c['e']:5d} {c['p']:4d}  {key[2]:12s} {key[1]}")

# 현재 구조 키워드 중 창 안에서 한 번도 안 걸린 것
print("\n## 지금 트리 키워드 중 창 안에서 연결 0")
hit_paths = {(r["sid"], r["path"]) for r in rows}
zero = Counter()
total = Counter()
for r in con.execute("select stock_id, path, kind from keywords"):
    s = stocks[r["stock_id"]]
    if s["kind"] == "topic":
        continue
    nm = s["name_ko"] or s["name"]
    total[(nm, r["kind"])] += 1
    if (r["stock_id"], r["path"]) not in hit_paths:
        zero[(nm, r["kind"])] += 1
for k in sorted(total):
    print(k, f"{zero[k]}/{total[k]}")

# 키워드로만 통과한 기사 제목
print("\n## 키워드 경로로만 통과한 기사")
titles = {r["aid"]: (r["title"], r["pub"]) for r in rows}
for a in sorted(kw_only, key=lambda a: titles[a][1] or 0, reverse=True):
    s = by_article[a]
    tags = "; ".join(f"{stocks[sid]['name_ko'] or stocks[sid]['name']}<{k}>" for k, sid in s)
    others = sorted(by_article_all[a] - {k for k, _ in s})
    print(f"- {titles[a][0]}  [{tags}]  (그 밖 연결: {', '.join(others) or '없음'})")
