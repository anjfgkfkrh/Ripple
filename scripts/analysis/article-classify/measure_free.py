"""measure 무료 수치 a~d. 사용: python3 -I measure_free.py <DB 사본> > free.txt
"최근 7일"은 사본의 가장 늦은 articles.published_at부터 7일. 화면 기준은 사본 settings 값(없으면 6.0·5.0).
같은 소식은 similar_title.rs를 옮긴 규칙(24시간 안, 앞선 기사)으로 본다. 표본 ①·④는 labeled-2026-11-13.txt와 (종목, 제목)으로 맞춘다.
"""
import re
import sqlite3
import sys
from collections import Counter, defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
db = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
DAY = 86400000


def setting(key, default):
    r = db.execute("SELECT value FROM settings WHERE key=?", (key,)).fetchone()
    return float(r[0]) if r else default


MINREL, MINMAG = setting("evaluation.minRelevance", 6.0), setting("evaluation.minMagnitude", 5.0)
SHOWN = "e.relevance >= ? AND (e.magnitude IS NULL OR e.magnitude >= ?) AND s.deleted_at IS NULL"
SHOWN_ARGS = (MINREL, MINMAG)
latest = db.execute("SELECT MAX(published_at) FROM articles").fetchone()[0]
since = latest - 7 * DAY
kst = lambda ms: db.execute("SELECT datetime(?/1000,'unixepoch','+9 hours')", (ms,)).fetchone()[0]

# ---- similar_title 옮김 ----
PREFIX = re.compile(r"^[\s﻿]*(?:[\[【(][^\]】)]{1,12}[\]】)][\s﻿]*)+")
BREAK = re.compile(r"…|⋯|\.\.\.|···| - | \| |—")


def norm(t):
    return [c for c in t.lower() if ("a" <= c <= "z") or ("0" <= c <= "9") or ("가" <= c <= "힣")]


def bigrams(cs):
    return {cs[i] + cs[i + 1] for i in range(len(cs) - 1)}


def feats(title):
    body = PREFIX.sub("", title, count=1)
    lead = BREAK.split(body)[0]
    return bigrams(norm(body)), bigrams(norm(lead))


def jac(a, b):
    if not a or not b:
        return 0.0
    s = len(a & b)
    return s / (len(a) + len(b) - s)


def same(fa, fb):
    return jac(fa[1], fb[1]) >= 0.5 or jac(fa[0], fb[0]) >= 0.4


print(f"기준: 관련도 >= {MINREL}, 영향도 없음 또는 >= {MINMAG}. 사본 최신 발행 {kst(latest)} (한국 시간), 7일 시작 {kst(since)}")
print(f"사본 기사 {db.execute('SELECT COUNT(*) FROM articles').fetchone()[0]}건")

# ---- a ----
stored = db.execute("SELECT COUNT(*) FROM articles WHERE published_at >= ?", (since,)).fetchone()[0]
evaluated = db.execute(
    "SELECT COUNT(DISTINCT article_id) FROM evaluations e JOIN articles a ON a.id=e.article_id WHERE a.published_at >= ?", (since,)
).fetchone()[0]
shown_rows = db.execute(
    f"""SELECT DISTINCT a.id, a.published_at, a.title FROM evaluations e
        JOIN articles a ON a.id=e.article_id JOIN stocks s ON s.id=e.stock_id
        WHERE a.published_at >= ? AND {SHOWN} ORDER BY a.published_at""",
    (since, *SHOWN_ARGS),
).fetchall()
F = [(i, p, feats(t)) for i, p, t in shown_rows]
dup = 0
for k, (i, p, f) in enumerate(F):
    if any(p - q <= DAY and same(f, g) for j, q, g in F[:k] if q <= p and j != i):
        dup += 1
shown = len(shown_rows)
print("\n## a. 최근 7일 (합계 / 하루 평균, 7로 나눔)")
print("| 항목 | 합계 | 하루 평균 |\n|---|---|---|")
for name, v in (("저장 기사", stored), ("평가가 하나라도 있는 기사", evaluated), ("화면 기준을 넘는 평가가 있는 기사", shown),
                ("그 기사 중 같은 소식(24시간 안, similar_title 규칙) 앞선 기사가 있는 수", dup)):
    print(f"| {name} | {v} | {v/7:.1f} |")
print(f"(같은 소식 비교 대상은 화면 기준을 넘는 기사끼리. similar_title.rs를 그대로 옮겨 썼다)")

# ---- b ----
print("\n## b. 성격 물을 건수와 비용")
print(f"최근 7일 화면 기준 넘는 기사 {shown}건 × 700토큰 × $0.042/100만 = ${shown*700*0.042/1e6:.4f} (하루 평균 {shown/7:.1f}건 → ${shown/7*700*0.042/1e6:.5f}/일)")

# ---- c ----
lab = []
for line in (HERE / "labeled-2026-11-13.txt").read_text().splitlines():
    if line.startswith("##") or not line.strip():
        continue
    p = line.split(" ", 6)
    lab.append((p[0], p[1], p[6].strip()))
labkey = {(n, t): m for m, n, t in lab if m in "①④"}
rows = db.execute(
    f"""SELECT COALESCE(s.name_ko, s.name), e.relevance, e.magnitude, a.title, a.id,
               (SELECT MAX(e2.relevance) FROM evaluations e2 JOIN stocks s2 ON s2.id=e2.stock_id
                 WHERE e2.article_id=a.id AND s2.kind='market')
          FROM evaluations e JOIN stocks s ON s.id=e.stock_id JOIN articles a ON a.id=e.article_id
         WHERE s.kind='stock' AND {SHOWN} ORDER BY 1, e.relevance""",
    SHOWN_ARGS,
).fetchall()
hit = [r for r in rows if r[5] is not None and r[5] > r[1]]
print("\n## c. 지난 평가 규칙(종목 평가가 화면 기준을 넘고, 같은 기사의 시장 평가 관련도가 더 큼, 기간 전체)")
print(f"종목 평가 중 화면 기준을 넘는 것 {len(rows)}건, 그중 시장 관련도가 더 큰 것 {len(hit)}건")
print("| 종목 | 숨는 수 |\n|---|---|")
for n, c in Counter(r[0] for r in hit).most_common():
    print(f"| {n} | {c} |")
inlab = [r for r in hit if (r[0], r[3]) in labkey]
print(f"표본(①·④) 제목과 같은 것 {len(inlab)}건 (표본 ①·④ {len(labkey)}건 중). ①로 맞은 {sum(labkey[(r[0], r[3])] == '①' for r in inlab)}, ④로 맞은 {sum(labkey[(r[0], r[3])] == '④' for r in inlab)}")
notin = [r for r in hit if (r[0], r[3]) not in labkey]
print(f"표본에 없는 것 {len(notin)}건" + (f" (50건만 보임)" if len(notin) > 50 else " (전부)"))
print("| 종목 | 종목 관련도 | 시장 관련도 | 제목 |\n|---|---|---|---|")
for n, rel, mag, t, aid, mr in notin[:50]:
    print(f"| {n} | {rel:.1f} | {mr:.1f} | {t} |")

# ---- d ----
ALIAS = {
    "KR": ["코스피", "코스닥", "kospi", "kosdaq", "국내 증시", "국내증시", "한국 증시"],
    "US": ["나스닥", "s&p500", "s&p 500", "다우지수", "다우존스", "뉴욕증시", "미국 증시", "월가"],
}
CMP = "다우"


def squash(t):
    return re.sub(r"\s+", " ", t.lower()).strip()


def has(text, phrase):
    if not re.fullmatch(r"[a-z0-9&\s]+", phrase):
        return phrase in text
    for m in re.finditer(re.escape(phrase), text):
        a, b = m.start(), m.end()
        if (a == 0 or not text[a - 1].isascii() or not text[a - 1].isalnum()) and (
            b == len(text) or not text[b].isascii() or not text[b].isalnum()
        ):
            return True
    return False


arts = db.execute("SELECT id, title, summary, published_at FROM articles WHERE published_at >= ? ORDER BY published_at DESC", (since,)).fetchall()
linked = defaultdict(set)
for aid, mk in db.execute("SELECT m.article_id, s.market FROM article_matches m JOIN stocks s ON s.id=m.stock_id WHERE s.kind='market'"):
    linked[mk].add(aid)
print("\n## d. 별칭 걸림 (최근 7일 기사 %d건, 제목+요약)" % len(arts))
print("시장 항목:", db.execute("SELECT group_concat(market||':'||COALESCE(name_ko,name), ', ') FROM stocks WHERE kind='market'").fetchone()[0])
found = {}
cnt = Counter()
for aid, title, summ, p in arts:
    text = squash(title + " " + (summ or ""))
    hits = {a for al in (*ALIAS["KR"], *ALIAS["US"], CMP) for a in [al] if has(text, al)}
    found[aid] = (title, hits)
    cnt.update(hits)
print("| 별칭 | 기사 수 |\n|---|---|")
for mk in ("KR", "US"):
    for al in ALIAS[mk]:
        print(f"| {mk} {al} | {cnt[al]} |")
print(f"| 비교용 {CMP} | {cnt[CMP]} |")
tot_unlinked = 0
print("\n| 증시 | 별칭이 하나라도 걸린 기사 | 그중 그 증시 시장 항목 연결 없음 |\n|---|---|---|")
for mk in ("KR", "US"):
    ids = [a for a, (t, h) in found.items() if h & set(ALIAS[mk])]
    un = [a for a in ids if a not in linked[mk]]
    tot_unlinked += len(un)
    print(f"| {mk} | {len(ids)} | {len(un)} |")
print(f"연결 없는 수 합 {tot_unlinked}건 × $0.000038 = ${tot_unlinked*0.000038:.4f}")
dau = [t for a, (t, h) in found.items() if CMP in h and not ({"다우지수", "다우존스"} & h)]
print(f"\n\"다우\"만 걸리고 다우지수·다우존스는 아닌 기사 {len(dau)}건, 제목 20건:")
for t in dau[:20]:
    print("-", t)
wol = [t for a, (t, h) in found.items() if "월가" in h and not (set(ALIAS["US"]) - {"월가"}) & h]
print(f"\n\"월가\"로만 걸린 기사 {len(wol)}건, 제목 20건:")
for t in wol[:20]:
    print("-", t)
