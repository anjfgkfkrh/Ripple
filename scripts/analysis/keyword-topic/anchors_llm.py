"""LLM이 낸 별칭·고유어로 앵커 규칙을 다시 돌린다(14일, 종목 키워드 연결). 별칭은 제목+요약, 고유어는 제목에서 찾는다."""
import json, sqlite3, sys
conn = sqlite3.connect("file:db/ripple.db?mode=ro", uri=True); conn.row_factory = sqlite3.Row
mx = conn.execute("SELECT max(evaluated_at) FROM evaluations").fetchone()[0]
rows = conn.execute("""SELECT e.relevance r, e.magnitude m, e.stock_id sid, am.keyword_path path, a.title, coalesce(a.summary,'') summary
 FROM evaluations e JOIN article_matches am ON am.id=e.match_id JOIN stocks s ON s.id=e.stock_id JOIN articles a ON a.id=e.article_id
 WHERE e.magnitude IS NOT NULL AND e.copied_from IS NULL AND s.kind='stock' AND am.match_kind='keyword' AND e.evaluated_at>=?""", (mx - 14*86400000,)).fetchall()
names = {1: "삼성전자", 3: "현대건설", 4: "두산에너빌리티"}
hand = {1: (["삼전", "삼성"], "반도체|메모리|hbm|d램|낸드|파운드리|갤럭시|하이닉스|엔비디아|tsmc|스마트폰".split("|")),
        3: (["현건", "힐스테이트", "디에이치"], "건설|원전|재건축|정비사업|분양|주택|플랜트|수주|택지".split("|")),
        4: (["두산"], "원전|원자력|smr|가스터빈|웨스팅하우스|한수원|체코|팀코리아|터빈".split("|"))}
def sim(label, get):
    tc = tp = kc = kp = 0; lost = []
    for sid, nm in names.items():
        al, an = get(sid); al = [x.lower() for x in al]; an = [x.lower().replace(" ", "") for x in an]
        for r in rows:
            if r["sid"] != sid: continue
            p = r["r"] >= 6 and r["m"] >= 6; tc += 1; tp += p
            t = r["title"].lower(); ts = (r["title"] + " " + r["summary"]).lower()
            if any(a in ts for a in al) or any(a in t.replace(" ", "") for a in an):
                kc += 1; kp += p
            elif p: lost.append(f"{nm[:2]}: {r['title'][:38]}")
    print(f"{label}: 호출 {tc} → {kc} ({100*(tc-kc)/tc:.0f}% 절감), 통과 {tp} → {kp}")
    return lost
sim("손으로 정한 앵커", lambda sid: hand[sid])
for r in (1, 2):
    def get(sid, r=r):
        o = json.loads(json.load(open(f"paid/res/or/m2_v1_{sid}_r{r}.json"))["choices"][0]["message"]["content"])
        return o["aliases"], o["anchors"]
    lost = sim(f"LLM 새 형식 r{r}", get)
    print("   놓침:", "; ".join(lost[:14]))
def both(sid):
    a, b = [json.loads(json.load(open(f"paid/res/or/m2_v1_{sid}_r{r}.json"))["choices"][0]["message"]["content"]) for r in (1, 2)]
    return a["aliases"] + b["aliases"], a["anchors"] + b["anchors"]
lost = sim("LLM r1+r2 합친 고유어", both); print("   놓침:", "; ".join(lost[:14]))
def with_l1(sid):
    o = json.loads(json.load(open(f"paid/res/or/m2_v1_{sid}_r1.json"))["choices"][0]["message"]["content"])
    return o["aliases"], o["anchors"] + [k["keyword"] for k in o["keywords"] if k["level"] == 1 and " " not in k["keyword"]]
lost = sim("LLM r1 고유어 + 한 낱말 1단계", with_l1); print("   놓침:", "; ".join(lost[:14]))
