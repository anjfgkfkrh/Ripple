"""본 DB 백업 사본(읽기 전용)으로 재는 것: 키워드별 수율, 제목 적중, 수율 게이트 시뮬레이션, 토픽 KR/US 중복."""
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
SELECT e.id, e.match_id, e.article_id, e.stock_id, e.relevance r, e.magnitude m, e.copied_from, e.evaluated_at at,
       am.match_kind mk, am.level, am.keyword_path path, s.kind skind, s.market, s.topic_id,
       coalesce(s.name_ko, s.name) nm, a.title, coalesce(a.summary,'') summary, a.published_at
FROM evaluations e JOIN article_matches am ON am.id=e.match_id JOIN stocks s ON s.id=e.stock_id
JOIN articles a ON a.id=e.article_id
WHERE e.magnitude IS NOT NULL AND e.evaluated_at >= ? ORDER BY e.evaluated_at, e.id
""",
    (since,),
).fetchall()
auto_topic_names = {r[0] for r in conn.execute("SELECT name FROM topics WHERE source='auto'")}

WS = re.compile(r"\s+")
EDGE = re.compile(r"^[^\w]+|[^\w]+$")
ASCII = re.compile(r"^[a-z0-9][a-z0-9.&+'-]*$")


def norm(t):
    return WS.sub(" ", t.lower())


def words(t):
    return [w for w in (EDGE.sub("", x) for x in norm(t).split(" ")) if w]


def has(hay, w):
    if not ASCII.match(w):
        return w in hay
    return re.search(r"(?<![a-z0-9])" + re.escape(w) + r"(?![a-z0-9])", hay) is not None


def hit(text, kw):
    h = norm(text)
    ws = words(kw)
    return bool(ws) and all(has(h, w) for w in ws)


def group(r):
    if r["skind"] == "stock":
        if r["mk"] != "keyword":
            return "종목-이름"
        segs = r["path"].split(" → ")
        if len(segs) > 1 and segs[1] in auto_topic_names:
            return "종목-토픽가지"
        return "종목-구조키워드"
    if r["skind"] == "market":
        return "시장-" + r["market"]
    return "자동토픽-" + r["market"]


E = []
for r in rows:
    d = dict(r)
    d["grp"] = group(r)
    d["pass"] = r["r"] >= 6 and r["m"] >= 6
    d["called"] = r["copied_from"] is None
    d["leaf"] = r["path"].split(" → ")[-1]
    E.append(d)

print(f"기간 {DAYS}일, 평가 {len(E)}건, Jev 호출 {sum(e['called'] for e in E)}건")
print("\n== 1. 묶음별 깔때기 ==")
agg = defaultdict(lambda: [0, 0, 0, 0])
for e in E:
    a = agg[e["grp"]]
    a[0] += 1
    a[1] += e["called"]
    a[2] += e["pass"]
    a[3] += e["pass"] and e["called"]
tot_calls = sum(a[1] for a in agg.values())
for g, a in sorted(agg.items()):
    print(f"{g:14s} 평가 {a[0]:6d} 호출 {a[1]:6d}({100*a[1]/tot_calls:4.1f}%) 통과 {a[2]:5d} ({100*a[2]/a[0]:4.1f}%) 통과1건당 호출 {a[1]/max(1,a[3]):6.1f}")

print("\n== 2. 제목 적중 vs 요약에만 적중(키워드 연결) ==")
agg = defaultdict(lambda: [0, 0, 0, 0])
for e in E:
    if e["mk"] != "keyword":
        continue
    where = "제목" if hit(e["title"], e["leaf"]) else ("요약만" if hit(e["title"] + " " + e["summary"], e["leaf"]) else "재현안됨")
    a = agg[(e["grp"], where)]
    a[0] += 1
    a[1] += e["called"]
    a[2] += e["pass"]
for k, a in sorted(agg.items()):
    print(f"{k[0]:14s} {k[1]:6s} 평가 {a[0]:6d} 호출 {a[1]:6d} 통과 {a[2]:5d} ({100*a[2]/a[0]:4.1f}%)")

print("\n== 3. 종목 구조키워드: 키워드(종목+끝 낱말)별 수율 집중도 ==")
kw = defaultdict(lambda: [0, 0])
for e in E:
    if e["grp"] == "종목-구조키워드" and e["called"]:
        k = kw[(e["nm"], e["leaf"])]
        k[0] += 1
        k[1] += e["pass"]
tc = sum(v[0] for v in kw.values())
tp = sum(v[1] for v in kw.values())
zero = [(k, v) for k, v in kw.items() if v[1] == 0]
print(f"키워드 {len(kw)}개, 호출 {tc}, 통과 {tp}. 통과 0인 키워드 {len(zero)}개가 호출 {sum(v[0] for _, v in zero)}건({100*sum(v[0] for _, v in zero)/tc:.1f}%)")
print("호출 많은 순 25개(호출/통과):")
for k, v in sorted(kw.items(), key=lambda x: -x[1][0])[:25]:
    print(f"  {k[0]} · {k[1]}: {v[0]}/{v[1]}")
print("통과 있는 키워드(통과율순):")
for k, v in sorted([x for x in kw.items() if x[1][1] > 0], key=lambda x: -x[1][1] / x[1][0])[:40]:
    print(f"  {k[0]} · {k[1]}: {v[0]}/{v[1]}")


def simulate(label, pred, keyf, ns=(5, 10, 20, 30)):
    """수율 게이트: 같은 열쇠로 앞서 Jev를 n번 불렀는데 통과가 0이면 그 뒤로는 부르지 않는다."""
    evs = [e for e in E if pred(e) and e["called"]]
    base_calls = len(evs)
    base_pass = sum(e["pass"] for e in evs)
    print(f"\n== 게이트 시뮬레이션: {label} (기준 호출 {base_calls}, 통과 {base_pass}) ==")
    for n in ns:
        seen = defaultdict(lambda: [0, 0])
        calls = lost = 0
        for e in evs:
            s = seen[keyf(e)]
            if s[0] >= n and s[1] == 0:
                lost += e["pass"]
                continue
            calls += 1
            s[0] += 1
            s[1] += e["pass"]
        print(f"  n={n:3d}: 호출 {calls}({100*(base_calls-calls)/base_calls:4.1f}% 절감), 놓친 통과 {lost}({100*lost/max(1,base_pass):4.1f}%)")


simulate("종목 구조키워드(종목+끝 낱말)", lambda e: e["grp"] == "종목-구조키워드", lambda e: (e["nm"], e["leaf"]))
simulate("종목 토픽가지(종목+토픽)", lambda e: e["grp"] == "종목-토픽가지", lambda e: (e["nm"], e["path"].split(" → ")[1]))
simulate("자동토픽 KR(토픽)", lambda e: e["grp"] == "자동토픽-KR", lambda e: e["stock_id"])
simulate("자동토픽 US(토픽)", lambda e: e["grp"] == "자동토픽-US", lambda e: e["stock_id"])
simulate("자동토픽 KR(토픽+검색어)", lambda e: e["grp"] == "자동토픽-KR", lambda e: (e["stock_id"], e["leaf"]))
simulate("시장 KR(끝 낱말)", lambda e: e["grp"] == "시장-KR", lambda e: e["leaf"])
simulate("시장 US(끝 낱말)", lambda e: e["grp"] == "시장-US", lambda e: e["leaf"])

print("\n== 4. 자동토픽 KR/US 같은 기사·같은 토픽 쌍 ==")
by = defaultdict(dict)
for e in E:
    if e["skind"] == "topic" and e["topic_id"]:
        by[(e["article_id"], e["topic_id"])][e["market"]] = e
pairs = [(v["KR"], v["US"]) for v in by.values() if "KR" in v and "US" in v]
both_called = [(k, u) for k, u in pairs if k["called"] and u["called"]]
n = len(both_called)
if n:
    import statistics as st

    dr = [abs(k["r"] - u["r"]) for k, u in both_called]
    print(f"쌍 {len(pairs)}(둘 다 호출 {n}). 관련도 차이 평균 {st.mean(dr):.2f}, 중앙 {st.median(dr):.2f}, 1 초과 {sum(d > 1 for d in dr)}건")
    rk = [k["r"] for k, u in both_called]
    ru = [u["r"] for k, u in both_called]
    mk_ = [k["m"] for k, u in both_called]
    mu = [u["m"] for k, u in both_called]
    print(f"상관: 관련도 {st.correlation(rk, ru):.3f}, 영향도 {st.correlation(mk_, mu):.3f}")
    kr6 = sum(k["r"] >= 6 for k, u in both_called)
    print(f"KR 관련도>=6: {kr6}건({100*kr6/n:.1f}%). KR 관련도<6인데 US 통과: {sum(k['r'] < 6 and u['pass'] for k, u in both_called)}건")
    print(f"통과: KR만 {sum(k['pass'] and not u['pass'] for k, u in both_called)}, US만 {sum(u['pass'] and not k['pass'] for k, u in both_called)}, 둘 다 {sum(k['pass'] and u['pass'] for k, u in both_called)}, 둘 다 아님 {sum(not k['pass'] and not u['pass'] for k, u in both_called)}")
    for thr in (4, 5, 5.5, 6):
        skip = [(k, u) for k, u in both_called if k["r"] < thr]
        print(f"  KR 관련도<{thr}면 US 생략: 생략 {len(skip)}({100*len(skip)/n:.1f}%), 놓친 US 통과 {sum(u['pass'] for k, u in skip)}")

print("\n== 5. 기사 하나가 걸린 자동토픽 수(KR 기준, 호출) ==")
cnt = defaultdict(int)
for e in E:
    if e["grp"] == "자동토픽-KR" and e["called"]:
        cnt[e["article_id"]] += 1
dist = defaultdict(int)
for v in cnt.values():
    dist[min(v, 6)] += 1
print({k: dist[k] for k in sorted(dist)}, "기사", len(cnt), "호출", sum(cnt.values()))
