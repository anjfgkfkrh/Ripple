"""반사실 비교. 사용: python3 -I counterfactual.py <ripple.db> <시작 KST>"""
import json
import re
import sqlite3
import sys
from collections import Counter, defaultdict
from datetime import datetime, timedelta, timezone

KST = timezone(timedelta(hours=9))
DB = sys.argv[1]
START = int(datetime.strptime(sys.argv[2], "%Y-%m-%d %H:%M").replace(tzinfo=KST).timestamp() * 1000)
con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
con.row_factory = sqlite3.Row
stocks = {r["id"]: r for r in con.execute("select * from stocks")}
ALIASES = {
    "KR": ["코스피", "코스닥", "kospi", "kosdaq", "국내 증시", "국내증시", "한국 증시"],
    "US": ["나스닥", "s&p500", "s&p 500", "다우지수", "다우존스", "뉴욕증시", "미국 증시", "월가"],
}
WS = re.compile(r"\s+")


def norm(t):
    return WS.sub(" ", (t or "").lower())


def has_phrase(h, p):
    for m in re.finditer(re.escape(p), h):
        a, b = m.start(), m.end()
        if p[0].isascii() and p[0].isalnum() and a > 0 and h[a - 1].isascii() and h[a - 1].isalnum():
            continue
        if p[-1].isascii() and p[-1].isalnum() and b < len(h) and h[b].isascii() and h[b].isalnum():
            continue
        return True
    return False


def passed(r):
    return r["rel"] is not None and r["rel"] >= 6 and (r["mag"] is None or r["mag"] >= 5)


rows = con.execute(
    """select m.article_id aid, m.stock_id sid, m.match_kind mk, m.gated g, m.keyword_path path,
              e.relevance rel, e.magnitude mag, e.copied_from cp, a.title, a.summary
       from article_matches m join articles a on a.id=m.article_id left join evaluations e on e.match_id=m.id
       where m.created_at >= ?""",
    (START,),
).fetchall()

print("## 1. 시장 키워드 연결 중 18단계 고정 별칭이 제목+요약에 있는 것(별칭으로도 걸렸을 것)")
for mkt in ("KR", "US"):
    c = Counter()
    for r in rows:
        s = stocks[r["sid"]]
        if s["kind"] != "market" or s["market"] != mkt:
            continue
        h = norm(f"{r['title']} {r['summary']}")
        al = any(has_phrase(h, p) for p in ALIASES[mkt])
        c["match"] += 1
        c["alias"] += al
        if r["cp"] is None and r["rel"] is not None:
            c["eval"] += 1
            c["eval_alias"] += al
        if passed(r):
            c["pass"] += 1
            c["pass_alias"] += al
    print(mkt, dict(c))

# 별칭 경로만 있었다면 걸렸을 기사 수(시장 키워드 연결 여부와 무관): 창 안 전체 기사
arts = con.execute("select id, title, summary from articles where collected_at >= ?", (START,)).fetchall()
for mkt in ("KR", "US"):
    n = sum(any(has_phrase(norm(f"{a['title']} {a['summary']}"), p) for p in ALIASES[mkt]) for a in arts)
    print(f"{mkt} 창 안 기사 {len(arts)} 중 별칭으로 걸릴 기사 {n}")

print("\n## 2. 종목: 고유어가 제목에 있는 기사(이름·별칭 없음) — 트리 없이 고유어만으로 걸면")
for sid, s in stocks.items():
    if s["kind"] != "stock":
        continue
    anchors = [a.lower().replace(" ", "") for a in json.loads(s["anchors"]) if a.strip()]
    names = [norm(s["name_ko"] or s["name"])] + [norm(a) for a in json.loads(s["aliases"])]
    kw_rows = {r["aid"]: r for r in rows if r["sid"] == sid and r["mk"] == "keyword"}
    name_aids = {r["aid"] for r in rows if r["sid"] == sid and r["mk"] in ("name", "direct")}
    hit = []
    for a in arts:
        t = (a["title"] or "").lower().replace(" ", "")
        if any(x in t for x in anchors):
            if a["id"] in name_aids:
                continue
            hit.append(a["id"])
    in_tree = [a for a in hit if a in kw_rows and not kw_rows[a]["g"]]
    in_tree_pass = [a for a in in_tree if passed(kw_rows[a])]
    not_tree = [a for a in hit if a not in kw_rows]
    print(
        f"{s['name_ko']}: 고유어 {len(anchors)}개, 이름 없이 제목에 고유어 {len(hit)}건, 그중 트리 키워드로도 걸려 평가 {len(in_tree)}(통과 {len(in_tree_pass)}), 트리에 안 걸림 {len(not_tree)}"
    )
    print("   고유어:", json.loads(s["anchors"]))
    titles = {a["id"]: a["title"] for a in arts}
    for a in not_tree[:12]:
        print("   - (트리 밖)", titles[a])

print("\n## 3. 비용(창 안)")
W = START
for r in con.execute(
    "select purpose, count(*) n, sum(cost_usd) c from usage_logs where at>=? group by 1", (W,)
):
    print(r["purpose"], r["n"], round(r["c"], 4))
span_h = (con.execute("select max(at) from usage_logs").fetchone()[0] - W) / 3600000
print(f"창 길이 {span_h:.1f}시간")
