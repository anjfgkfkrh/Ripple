"""앵커(종목 고유어) 동시 출현 규칙, 통과 0 토픽에 쓴 호출, 토픽 수명, 눈가림 표본 뽑기."""
import json
import random
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
       am.match_kind mk, am.level, am.keyword_path path, s.kind skind, s.market, s.topic_id,
       coalesce(s.name_ko, s.name) nm, a.title, coalesce(a.summary,'') summary, a.published_at, a.publisher
FROM evaluations e JOIN article_matches am ON am.id=e.match_id JOIN stocks s ON s.id=e.stock_id
JOIN articles a ON a.id=e.article_id
WHERE e.magnitude IS NOT NULL AND e.evaluated_at >= ? ORDER BY e.evaluated_at, e.id
""",
    (since,),
).fetchall()
E = [dict(r) for r in rows]
for e in E:
    e["pass"] = e["r"] >= 6 and e["m"] >= 6
    e["called"] = e["copied_from"] is None
calls_total = sum(e["called"] for e in E)
pass_total = sum(e["pass"] and e["called"] for e in E)

print("== 1. 종목 키워드 연결에 앵커 동시 출현 규칙(손으로 정한 앵커로 가설만 확인) ==")
ALIAS = {"삼성전자": r"삼전|삼성", "현대건설": r"현대건설|현건|힐스테이트|디에이치", "두산에너빌리티": r"두산"}
DOMAIN = {
    "삼성전자": r"반도체|메모리|hbm|d램|낸드|파운드리|갤럭시|하이닉스|엔비디아|tsmc|스마트폰",
    "현대건설": r"건설|원전|재건축|정비사업|분양|주택|플랜트|수주|택지",
    "두산에너빌리티": r"원전|원자력|smr|가스터빈|웨스팅하우스|한수원|체코|팀코리아|터빈",
}
tot = [0, 0, 0, 0, 0, 0]
for nm in ALIAS:
    a_rx = re.compile(ALIAS[nm], re.I)
    d_rx = re.compile(DOMAIN[nm], re.I)
    kw = [e for e in E if e["nm"] == nm and e["mk"] == "keyword" and e["called"]]
    p = sum(e["pass"] for e in kw)
    a = [e for e in kw if a_rx.search(e["title"] + " " + e["summary"])]
    # 걸린 키워드 자신이 앵커 낱말이면 '동시 출현'이 아니므로, 제목에 앵커가 있는지로 본다
    d_title = [e for e in kw if a_rx.search(e["title"] + " " + e["summary"]) or d_rx.search(e["title"])]
    d_any = [e for e in kw if a_rx.search(e["title"] + " " + e["summary"]) or d_rx.search(e["title"] + " " + e["summary"])]
    print(f"{nm}: 지금 호출 {len(kw)}·통과 {p} | 별칭 있음 {len(a)}·{sum(e['pass'] for e in a)} | 별칭 또는 제목에 분야 앵커 {len(d_title)}·{sum(e['pass'] for e in d_title)} | 별칭 또는 제목·요약에 분야 앵커 {len(d_any)}·{sum(e['pass'] for e in d_any)}")
    for i, v in enumerate((len(kw), p, len(a), sum(e["pass"] for e in a), len(d_title), sum(e["pass"] for e in d_title))):
        tot[i] += v
    lost = [e for e in kw if e["pass"] and e not in d_title]
    for e in lost[:8]:
        print(f"     놓침: [{e['path']}] {e['title'][:60]} ({e['r']:.1f}/{e['m']:.1f})")
print(f"합계: 지금 {tot[0]}·{tot[1]} → 별칭만 {tot[2]}·{tot[3]} → 별칭 또는 제목 앵커 {tot[4]}·{tot[5]} (전체 호출 {calls_total}의 {100*(tot[0]-tot[4])/calls_total:.1f}% 절감)")

print("\n== 2. 자동 토픽별 수율: 통과 0 토픽에 쓴 호출 ==")
tp = defaultdict(lambda: [0, 0, 0])
for e in E:
    if e["skind"] == "topic" and e["topic_id"] and e["called"]:
        t = tp[e["topic_id"]]
        t[0] += 1
        t[1] += e["pass"]
        t[2] += e["market"] == "KR" and e["pass"]
names = {r[0]: r[1] for r in conn.execute("SELECT id, name FROM topics")}
zero = {k: v for k, v in tp.items() if v[1] == 0}
low = {k: v for k, v in tp.items() if v[1] > 0 and v[1] / v[0] < 0.03}
tc = sum(v[0] for v in tp.values())
print(f"14일 동안 평가가 있던 자동 토픽 {len(tp)}개, 호출 {tc}. 통과 0인 토픽 {len(zero)}개가 호출 {sum(v[0] for v in zero.values())}({100*sum(v[0] for v in zero.values())/tc:.1f}%). 통과율 3% 미만 {len(low)}개가 호출 {sum(v[0] for v in low.values())}")
print("호출 많은 통과 0 토픽:")
for k, v in sorted(zero.items(), key=lambda x: -x[1][0])[:12]:
    print(f"   {names.get(k)}: {v[0]}")
print("통과 많은 토픽(호출/통과):")
for k, v in sorted(tp.items(), key=lambda x: -x[1][1])[:10]:
    print(f"   {names.get(k)}: {v[0]}/{v[1]}")
# 토픽마다 처음 N번 호출의 통과로 나머지를 예측할 수 있는가
print("토픽 수율의 지속성: 처음 20번 호출에서 통과가 있었던 토픽 vs 없었던 토픽의 그 뒤 수율")
seq = defaultdict(list)
for e in E:
    if e["skind"] == "topic" and e["topic_id"] and e["called"] and e["market"] == "KR":
        seq[e["topic_id"]].append(e["pass"])
hot = [s for s in seq.values() if len(s) > 20 and any(s[:20])]
cold = [s for s in seq.values() if len(s) > 20 and not any(s[:20])]
for label, g in (("처음 20번에 통과 있음", hot), ("처음 20번에 통과 없음", cold)):
    c = sum(len(s) - 20 for s in g)
    p = sum(sum(s[20:]) for s in g)
    print(f"   {label}: 토픽 {len(g)}개, 그 뒤 호출 {c}, 통과 {p} ({100*p/max(1,c):.1f}%)")

print("\n== 3. 눈가림 표본(종목 이름 연결, 최근 7일, Jev 점수는 따로 저장) ==")
random.seed(20261110)
name_ev = [e for e in E if e["mk"] == "name" and e["called"] and e["at"] >= mx - 7 * 86400000]
bins = {
    "통과(관>=6·영>=6)": [e for e in name_ev if e["pass"]],
    "관>=6·영 5~6": [e for e in name_ev if e["r"] >= 6 and 5 <= e["m"] < 6],
    "관>=6·영 3~5": [e for e in name_ev if e["r"] >= 6 and 3 <= e["m"] < 5],
    "관>=6·영<3": [e for e in name_ev if e["r"] >= 6 and e["m"] < 3],
    "관<6": [e for e in name_ev if e["r"] < 6],
}
print({k: len(v) for k, v in bins.items()})
sample = []
for k, v in bins.items():
    for e in random.sample(v, min(12, len(v))):
        sample.append((k, e))
random.shuffle(sample)
out = sys.argv[2]
with open(out + "/blind_titles.txt", "w") as f:
    for i, (k, e) in enumerate(sample, 1):
        f.write(f"{i}. [{e['nm']}] {e['title']}\n   {e['summary'][:200]}\n")
with open(out + "/blind_key.json", "w") as f:
    json.dump([{"n": i, "bin": k, "r": e["r"], "m": e["m"], "impact": e["impact"], "nm": e["nm"]} for i, (k, e) in enumerate(sample, 1)], f, ensure_ascii=False)
print("표본", len(sample), "건 저장")
