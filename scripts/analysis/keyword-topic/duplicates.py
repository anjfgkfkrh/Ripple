"""토픽 KR/US 중복, 기사 단위 영향도 일관성, 종목 별칭·앵커, Jev 키워드 점수와 수율, 토픽 갱신 입력 줄이기."""
import json
import math
import re
import sqlite3
import sys
from collections import defaultdict

DB = sys.argv[1]
DAYS = 14
conn = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
conn.row_factory = sqlite3.Row
mx = conn.execute("SELECT max(evaluated_at) FROM evaluations").fetchone()[0]
since = mx - DAYS * 86400000
rows = conn.execute(
    """
SELECT e.id, e.article_id, e.stock_id, e.relevance r, e.magnitude m, e.impact, e.copied_from, e.evaluated_at at,
       am.match_kind mk, am.level, am.keyword_path path, am.created_at matched_at, s.kind skind, s.market, s.topic_id,
       coalesce(s.name_ko, s.name) nm, a.title, coalesce(a.summary,'') summary, a.published_at
FROM evaluations e JOIN article_matches am ON am.id=e.match_id JOIN stocks s ON s.id=e.stock_id
JOIN articles a ON a.id=e.article_id
WHERE e.magnitude IS NOT NULL AND e.evaluated_at >= ? ORDER BY e.evaluated_at, e.id
""",
    (since,),
).fetchall()
topics = conn.execute("SELECT id, name, terms, status, created_at, ended_at FROM topics WHERE source='auto'").fetchall()
topic_words = set()
for t in topics:
    topic_words.add(t["name"])
    topic_words.update(json.loads(t["terms"]))


def corr(x, y):
    n = len(x)
    mx_, my = sum(x) / n, sum(y) / n
    sxy = sum((a - mx_) * (b - my) for a, b in zip(x, y))
    sx = math.sqrt(sum((a - mx_) ** 2 for a in x))
    sy = math.sqrt(sum((b - my) ** 2 for b in y))
    return sxy / (sx * sy) if sx and sy else float("nan")


E = []
for r in rows:
    d = dict(r)
    d["pass"] = r["r"] >= 6 and r["m"] >= 6
    d["called"] = r["copied_from"] is None
    segs = r["path"].split(" → ")
    d["leaf"] = segs[-1]
    d["topicish"] = r["skind"] == "stock" and r["mk"] == "keyword" and (
        (len(segs) > 1 and segs[1] in topic_words) or segs[-1] in topic_words
    )
    E.append(d)
calls_total = sum(e["called"] for e in E)

print("== A. 자동토픽 KR/US 쌍 ==")
by = defaultdict(dict)
for e in E:
    if e["skind"] == "topic" and e["topic_id"]:
        by[(e["article_id"], e["topic_id"])][e["market"]] = e
bc = [(v["KR"], v["US"]) for v in by.values() if "KR" in v and "US" in v and v["KR"]["called"] and v["US"]["called"]]
n = len(bc)
print(f"둘 다 호출한 쌍 {n}. 상관: 관련도 {corr([k['r'] for k,u in bc],[u['r'] for k,u in bc]):.3f}, 영향도 {corr([k['m'] for k,u in bc],[u['m'] for k,u in bc]):.3f}")
print(f"통과: KR만 {sum(k['pass'] and not u['pass'] for k,u in bc)}, US만 {sum(u['pass'] and not k['pass'] for k,u in bc)}, 둘 다 {sum(k['pass'] and u['pass'] for k,u in bc)}, 둘 다 아님 {sum(not k['pass'] and not u['pass'] for k,u in bc)}")
for thr in (4, 5, 5.5, 6):
    skip = [(k, u) for k, u in bc if k["r"] < thr]
    print(f"  KR 관련도<{thr}면 US 생략: 생략 {len(skip)}({100*len(skip)/n:.1f}% = 전체 호출의 {100*len(skip)/calls_total:.1f}%), 놓친 US 통과 {sum(u['pass'] for k,u in skip)}/{sum(u['pass'] for k,u in bc)}")

print("\n== B. 같은 기사·같은 증시에서 토픽이 달라도 영향도가 같은가 ==")
for market in ("KR", "US"):
    g = defaultdict(list)
    for e in E:
        if e["skind"] in ("topic", "market") and e["market"] == market and e["called"]:
            g[e["article_id"]].append(e)
    multi = [v for v in g.values() if len(v) >= 2]
    spans = [max(x["m"] for x in v) - min(x["m"] for x in v) for v in multi]
    spans.sort()
    flips = sum(1 for v in multi if any(x["m"] >= 6 for x in v) and any(x["m"] < 6 for x in v))
    imp = sum(1 for v in multi if len({x["impact"] for x in v}) > 1)
    calls = sum(len(v) for v in g.values())
    print(f"{market}: 기사 {len(g)}건에 호출 {calls}(기사당 {calls/len(g):.2f}). 2개 이상 걸린 기사 {len(multi)}: 영향도 최대-최소 중앙 {spans[len(spans)//2]:.2f}, 90% {spans[int(len(spans)*0.9)]:.2f}, 6점 경계를 넘나드는 기사 {flips}({100*flips/len(multi):.1f}%), 호재/악재가 갈리는 기사 {imp}({100*imp/len(multi):.1f}%)")
# 시장+토픽을 기사 단위로 합치면
g = defaultdict(lambda: defaultdict(list))
for e in E:
    if e["skind"] in ("topic", "market") and e["called"]:
        g[e["article_id"]][e["market"]].append(e)
arts = len(g)
calls = sum(len(v) for a in g.values() for v in a.values())
tok_now = calls * 1008
# 가정(추정): 요청 바탕 190 + 기사 290, 점수 질문 하나 200, 선택 질문 130
tok_new = 0
for a in g.values():
    k = len({x["stock_id"] for v in a.values() for x in v if x["skind"] == "topic"} | set())
    topics_n = len({x["topic_id"] for v in a.values() for x in v if x["skind"] == "topic"})
    markets = len(a)
    has_market_item = any(x["skind"] == "market" for v in a.values() for x in v)
    tok_new += 190 + 290 + 200 * topics_n + (200 if has_market_item else 0) + markets * (200 + 130)
print(f"시장·토픽 평가: 기사 {arts}건, 지금 호출 {calls}(약 {tok_now/1e6:.2f}M 토큰). 기사당 한 요청으로 합치면 호출 {arts}, 추정 {tok_new/1e6:.2f}M 토큰({100*(1-tok_new/tok_now):.0f}% 절감, 가정: 바탕 190·기사 290·점수 질문 200·선택 질문 130)")

print("\n== C. 종목: 별칭·앵커 ==")
alias = {
    "삼성전자": r"삼전|삼성",
    "현대건설": r"현대건설|현건|힐스테이트|디에이치",
    "두산에너빌리티": r"두산에너빌|두산",
}
for nm, pat in alias.items():
    rx = re.compile(pat)
    kw = [e for e in E if e["nm"] == nm and e["mk"] == "keyword" and e["called"]]
    with_alias = [e for e in kw if rx.search(e["title"] + " " + e["summary"])]
    title_alias = [e for e in kw if rx.search(e["title"])]
    p = sum(e["pass"] for e in kw)
    print(f"{nm}: 키워드 연결 호출 {len(kw)} 통과 {p} | 제목·요약에 별칭({pat}) 있음: 호출 {len(with_alias)} 통과 {sum(e['pass'] for e in with_alias)} | 제목에 별칭: 호출 {len(title_alias)} 통과 {sum(e['pass'] for e in title_alias)} | 별칭 없음: 호출 {len(kw)-len(with_alias)} 통과 {p-sum(e['pass'] for e in with_alias)}")

print("\n== C2. 종목 키워드 연결을 토픽성(자동 토픽 이름·검색어)과 구조로 나눔 ==")
for label, pred in (("토픽성", lambda e: e["topicish"]), ("구조", lambda e: not e["topicish"])):
    kw = [e for e in E if e["skind"] == "stock" and e["mk"] == "keyword" and e["called"] and pred(e)]
    print(f"{label}: 호출 {len(kw)}({100*len(kw)/calls_total:.1f}%), 통과 {sum(e['pass'] for e in kw)}, 관련도>=6 {sum(e['r']>=6 for e in kw)}")
    lv = defaultdict(lambda: [0, 0])
    for e in kw:
        lv[e["level"]][0] += 1
        lv[e["level"]][1] += e["pass"]
    print("   단계별(호출/통과):", {k: tuple(v) for k, v in sorted(lv.items())})

print("\n== D. 지금 트리의 Jev 키워드 점수와 실제 수율(트리를 만든 뒤 걸린 기사) ==")
kws = conn.execute(
    "SELECT k.stock_id, k.level, k.text_ko, k.path, k.relevance, k.magnitude, k.created_at, k.topic_id, s.kind skind, coalesce(s.name_ko,s.name) nm "
    "FROM keywords k JOIN stocks s ON s.id=k.stock_id WHERE s.deleted_at IS NULL AND s.kind IN ('stock','market')"
).fetchall()
print("지금 키워드 수:", len(kws), "| 종류별:", dict(conn.execute("SELECT s.kind||CASE WHEN k.topic_id IS NULL THEN '' ELSE '-토픽가지' END, count(*) FROM keywords k JOIN stocks s ON s.id=k.stock_id WHERE s.deleted_at IS NULL AND s.kind IN ('stock','market') GROUP BY 1").fetchall()))
# 경로(path)가 같은 과거 평가를 모두 모은다(매일 다시 만들어도 같은 경로면 같은 키워드로 본다)
stat = defaultdict(lambda: [0, 0])
for e in E:
    if e["mk"] == "keyword" and e["called"]:
        s = stat[(e["stock_id"], e["path"])]
        s[0] += 1
        s[1] += e["pass"]
pts = []
for k in kws:
    if k["topic_id"] is not None or k["relevance"] is None:
        continue
    c, p = stat.get((k["stock_id"], k["path"]), (0, 0))
    pts.append((k["relevance"] + k["magnitude"], c, p, k["nm"], k["path"], k["relevance"], k["magnitude"], k["level"]))
pts.sort(reverse=True)
third = len(pts) // 3
for label, part in (("점수 상위 1/3", pts[:third]), ("중간 1/3", pts[third : 2 * third]), ("하위 1/3", pts[2 * third :])):
    c = sum(x[1] for x in part)
    p = sum(x[2] for x in part)
    print(f"  {label}: 키워드 {len(part)}개, 평균 점수합 {sum(x[0] for x in part)/len(part):.1f}, 14일 호출 {c}, 통과 {p}, 한 번도 안 걸린 키워드 {sum(x[1]==0 for x in part)}개")
print("  점수 분포(관련도):", sorted(round(x[5]) for x in pts)[:: max(1, len(pts) // 12)])
unused = sum(x[1] == 0 for x in pts)
print(f"  구조 키워드 {len(pts)}개 중 14일 동안 한 번도 기사에 안 걸린 것 {unused}개, 통과가 한 번이라도 있는 것 {sum(x[2]>0 for x in pts)}개")
lv = defaultdict(lambda: [0, 0, 0, 0])
for x in pts:
    a = lv[(x[3], x[7])]
    a[0] += 1
    a[1] += x[1]
    a[2] += x[2]
    a[3] += x[1] == 0
print("  항목·단계별(키워드 수/호출/통과/안 걸림):")
for k, v in sorted(lv.items()):
    print("   ", k, tuple(v))

print("\n== E. 날마다 트리가 얼마나 바뀌나(그날 기사에 걸린 구조 키워드 끝 낱말 기준, 근사) ==")
day = defaultdict(lambda: defaultdict(set))
for e in E:
    if e["skind"] in ("stock", "market") and e["mk"] == "keyword" and not e["topicish"]:
        d = (e["matched_at"] + 9 * 3600000 - 6 * 3600000) // 86400000  # 06:00 재생성 기준 하루
        day[e["nm"]][d].add(e["leaf"])
for nm, ds in day.items():
    keys = sorted(ds)
    js = []
    for a, b in zip(keys, keys[1:]):
        if b - a == 1:
            js.append(len(ds[a] & ds[b]) / len(ds[a] | ds[b]))
    allk = set().union(*ds.values())
    print(f"  {nm}: 14일 동안 걸린 서로 다른 구조 키워드 {len(allk)}개, 하루 평균 {sum(len(v) for v in ds.values())/len(ds):.1f}개, 이웃한 날 겹침(자카드) 평균 {sum(js)/max(1,len(js)):.2f}")
