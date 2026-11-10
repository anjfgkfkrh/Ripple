"""고유어를 자료에서 뽑을 수 있는지: 이름으로 걸려 통과한 기사 제목에 유난히 자주 나오는 낱말(배경: 보관 제목 전체)."""
import re, sqlite3, sys, math
from collections import Counter
conn = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
EDGE = re.compile(r"^[^\w]+|[^\w]+$"); JOSA = re.compile(r"(으로|에서|에게|까지|부터|이라|라는|은|는|이|가|을|를|의|에|도|와|과|로|만)$")
def toks(t):
    out = set()
    for w in re.split(r"[\s·…⋯,/'\"‘’“”\[\]()]+", t.lower()):
        w = EDGE.sub("", w)
        if len(w) >= 3: w = JOSA.sub("", w)
        if len(w) >= 2 and not w.isdigit(): out.add(w)
    return out
bg = Counter(); nbg = 0
for (t,) in conn.execute("SELECT title FROM feed_headlines"):
    nbg += 1; bg.update(toks(t))
mx = conn.execute("SELECT max(evaluated_at) FROM evaluations").fetchone()[0]
for sid, nm in ((1, "삼성전자"), (3, "현대건설"), (4, "두산에너빌리티")):
    rows = conn.execute("""SELECT DISTINCT a.title FROM evaluations e JOIN article_matches am ON am.id=e.match_id JOIN articles a ON a.id=e.article_id
      WHERE e.stock_id=? AND am.match_kind='name' AND e.relevance>=6 AND e.magnitude>=6 AND e.evaluated_at>=?""", (sid, mx - 14*86400000)).fetchall()
    fg = Counter()
    for (t,) in rows: fg.update(toks(t))
    n = len(rows); sc = []
    for w, c in fg.items():
        if c < max(3, n * 0.03): continue
        lift = (c / n) / ((bg[w] + 1) / nbg)
        sc.append((lift * math.log(c), w, c, bg[w]))
    sc.sort(reverse=True)
    print(f"{nm}: 통과한 이름 연결 제목 {n}개 →", ", ".join(f"{w}({c}/{b})" for _, w, c, b in sc[:22]))
