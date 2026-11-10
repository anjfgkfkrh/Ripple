"""시간 분리: 고유어는 앞 7일 자료로 만들고(LLM V1b), 뒤 7일의 종목 키워드 연결로 시험한다."""
import json, sqlite3
conn = sqlite3.connect("file:db/ripple.db?mode=ro", uri=True); conn.row_factory = sqlite3.Row
NOW = conn.execute("SELECT max(at) FROM usage_logs").fetchone()[0]
rows = conn.execute("""SELECT e.relevance r, e.magnitude m, e.stock_id sid, a.title, coalesce(a.summary,'') summary
 FROM evaluations e JOIN article_matches am ON am.id=e.match_id JOIN stocks s ON s.id=e.stock_id JOIN articles a ON a.id=e.article_id
 WHERE e.magnitude IS NOT NULL AND e.copied_from IS NULL AND s.kind='stock' AND am.match_kind='keyword' AND e.evaluated_at>=?""", (NOW - 7*86400000,)).fetchall()
names = {1: "삼성전자", 3: "현대건설", 4: "두산에너빌리티"}
hand = {1: (["삼전", "삼성"], "반도체|메모리|hbm|d램|낸드|파운드리|갤럭시|하이닉스|엔비디아|tsmc|스마트폰".split("|")),
        3: (["현건", "힐스테이트", "디에이치"], "건설|원전|재건축|정비사업|분양|주택|플랜트|수주|택지".split("|")),
        4: (["두산"], "원전|원자력|smr|가스터빈|웨스팅하우스|한수원|체코|팀코리아|터빈".split("|"))}
def llm(tag, r):
    def get(sid):
        o = json.loads(json.load(open(f"paid/res/or/m2_{tag}_{sid}_r{r}.json"))["choices"][0]["message"]["content"])
        return o["aliases"], o["anchors"]
    return get
def sim(label, get, show=False):
    out = []
    for sid, nm in names.items():
        al, an = get(sid); al = [x.lower() for x in al]; an = [x.lower().replace(" ", "") for x in an]
        tc = tp = kc = kp = 0; lost = []
        for r in rows:
            if r["sid"] != sid: continue
            p = r["r"] >= 6 and r["m"] >= 6; tc += 1; tp += p
            if any(a in (r["title"] + " " + r["summary"]).lower() for a in al) or any(a in r["title"].lower().replace(" ", "") for a in an): kc += 1; kp += p
            elif p: lost.append(r["title"][:34])
        out.append((nm, tc, tp, kc, kp, lost))
    T = [sum(o[i] for o in out) for i in (1, 2, 3, 4)]
    print(f"{label}: 호출 {T[0]} → {T[2]} ({100*(T[0]-T[2])/T[0]:.0f}% 절감), 통과 {T[1]} → {T[3]} | " + ", ".join(f"{o[0][:2]} {o[3]}/{o[4]}(원래 {o[1]}/{o[2]})" for o in out))
    if show:
        for o in out:
            if o[5]: print(f"    {o[0]} 놓침: " + " / ".join(o[5][:6]))
sim("손 앵커", lambda sid: hand[sid], True)
for r in (1, 2): sim(f"좁은 문구 V1 r{r}(참고 낱말 14일)", llm("v1", r))
for r in (1, 2): sim(f"넓힌 문구 V1b r{r}(참고 낱말 앞 7일)", llm("v1b", r), True)
