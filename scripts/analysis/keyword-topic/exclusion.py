"""유지·제외 규칙의 반대 경우 재기: (1) 기사는 적은데 통과하는 키워드 (2) 통과 0이던 키워드가 뒤에 통과하는 경우."""
import re, sqlite3, sys
from collections import defaultdict
conn = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True); conn.row_factory = sqlite3.Row
rows = conn.execute("""SELECT e.relevance r, e.magnitude m, e.evaluated_at at, am.keyword_path path, coalesce(s.name_ko,s.name) nm, a.title, coalesce(a.summary,'') summary
 FROM evaluations e JOIN article_matches am ON am.id=e.match_id JOIN stocks s ON s.id=e.stock_id JOIN articles a ON a.id=e.article_id
 WHERE e.magnitude IS NOT NULL AND e.copied_from IS NULL AND s.kind='stock' AND am.match_kind='keyword' ORDER BY e.evaluated_at""").fetchall()
E = [dict(r) | {"p": r["r"] >= 6 and r["m"] >= 6, "leaf": r["path"].split(" → ")[-1]} for r in rows]
t0, t1 = E[0]["at"], E[-1]["at"]
print(f"종목 키워드 연결 호출 {len(E)}, 통과 {sum(e['p'] for e in E)}, 기간 {(t1-t0)/86400000:.1f}일")
ALIAS = {"삼성전자": r"삼전|삼성", "현대건설": r"현대건설|현건|힐스테이트|디에이치", "두산에너빌리티": r"두산"}
DOMAIN = {"삼성전자": r"반도체|메모리|hbm|d램|낸드|파운드리|갤럭시|하이닉스|엔비디아|tsmc|스마트폰", "현대건설": r"건설|원전|재건축|정비사업|분양|주택|플랜트|수주|택지", "두산에너빌리티": r"원전|원자력|smr|가스터빈|웨스팅하우스|한수원|체코|팀코리아|터빈"}
def anchored(e):
    return re.search(ALIAS[e["nm"]], e["title"] + " " + e["summary"], re.I) or re.search(DOMAIN[e["nm"]], e["title"], re.I)
def anchor_word(e):
    m = re.search(DOMAIN[e["nm"]], e["title"], re.I)
    return (e["nm"], m.group(0).lower()) if m else (e["nm"], "별칭")

print("\n(1) 기사 수가 적은 키워드의 통과 몫(키워드 = 종목+끝 낱말, 전체 기간)")
kw = defaultdict(lambda: [0, 0])
for e in E:
    k = kw[(e["nm"], e["leaf"])]; k[0] += 1; k[1] += e["p"]
tp = sum(v[1] for v in kw.values())
for lo, hi in ((1, 5), (6, 20), (21, 100), (101, 10**9)):
    g = [v for v in kw.values() if lo <= v[0] <= hi]
    c = sum(v[0] for v in g); p = sum(v[1] for v in g)
    print(f"   호출 {lo}~{hi if hi < 10**9 else ''}회 키워드 {len(g)}개: 호출 {c}, 통과 {p}({100*p/tp:.0f}%), 통과율 {100*p/max(1,c):.1f}%")

def sim(label, evs, keyf, ns=(20, 50, 100)):
    print(f"\n(2) {label}: 주마다 그때까지 호출 N회 이상·통과 0인 열쇠를 다음 주부터 뺌 (기준 호출 {len(evs)}, 통과 {sum(e['p'] for e in evs)})")
    for n in ns:
        stat = defaultdict(lambda: [0, 0]); banned = set(); calls = lost = 0; lost_ex = []; week = 0
        for e in evs:
            w = (e["at"] - t0) // (7 * 86400000)
            if w != week:
                banned |= {k for k, v in stat.items() if v[0] >= n and v[1] == 0}; week = w
            k = keyf(e)
            if k in banned:
                lost += e["p"]
                if e["p"]: lost_ex.append(f"{k[1]}: {e['title'][:40]}")
                continue
            calls += 1; stat[k][0] += 1; stat[k][1] += e["p"]
        print(f"   N={n}: 뺀 열쇠 {len(banned)}개, 호출 {calls}({100*(len(evs)-calls)/len(evs):.0f}% 절감), 놓친 통과 {lost}")
        for x in lost_ex[:6]: print("        놓침:", x)

sim("앵커 조건 없이, 키워드(종목+끝 낱말)", E, lambda e: (e["nm"], e["leaf"]))
A = [e for e in E if anchored(e)]
sim("앵커 조건을 건 뒤, 걸린 고유어(종목+앵커 낱말)", A, anchor_word)
print("\n앵커 낱말별(호출/통과):")
st = defaultdict(lambda: [0, 0])
for e in A:
    s = st[anchor_word(e)]; s[0] += 1; s[1] += e["p"]
print("  ", ", ".join(f"{k[0][:2]}·{k[1]} {v[0]}/{v[1]}" for k, v in sorted(st.items(), key=lambda x: -x[1][0])))
