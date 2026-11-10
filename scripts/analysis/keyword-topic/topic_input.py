"""토픽 갱신 입력 줄이기 시뮬레이션(무료)과 종목 별칭으로 놓친 기사 세기."""
import json
import re
import sqlite3
import sys
from collections import defaultdict

DB = sys.argv[1]
conn = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
conn.row_factory = sqlite3.Row
H = conn.execute("SELECT id, title, publisher, feed, published_at, coalesce(summary,'') summary, url_key FROM feed_headlines ORDER BY published_at DESC, id DESC").fetchall()
now = max(h["published_at"] for h in H)
print(f"보관 제목 {len(H)}개, 기간 {(now-min(h['published_at'] for h in H))/3600000:.0f}시간")

PREFIX = re.compile(r"^[\s﻿]*(?:[\[【(][^\]】)]{1,12}[\]】)][\s﻿]*)+")
BREAK = re.compile(r"…|⋯|\.\.\.|···| - | \| |—")
KANA = re.compile(r"[぀-ヿ]")
HANGUL = re.compile(r"[가-힣]")


def nrm(t):
    return [c.lower() for c in t if (c.isascii() and c.isalnum()) or "가" <= c <= "힣"]


def bg(chars):
    return {chars[i] + chars[i + 1] for i in range(len(chars) - 1)}


def feats(title):
    body = PREFIX.sub("", title)
    lead = BREAK.split(body)[0]
    return bg(nrm(body)), bg(nrm(lead))


def jac(a, b):
    if not a or not b:
        return 0.0
    s = len(a & b)
    return s / (len(a) + len(b) - s)


def same(a, b):
    return jac(a[1], b[1]) >= 0.5 or jac(a[0], b[0]) >= 0.4


def est_tokens(titles):
    # 실측(2026-10-22): 제목 1,500개 = 약 60,475토큰 → 제목당 약 40토큰. 글자 수 비례로 나눈다.
    return sum(len(t) for t in titles) * (60475 / 1500) / 46.0


avg_len = sum(len(h["title"]) for h in H) / len(H)
print(f"제목 평균 글자 수 {avg_len:.1f}")

WS = re.compile(r"\s+")
EDGE = re.compile(r"^[^\w]+|[^\w]+$")
ASCII = re.compile(r"^[a-z0-9][a-z0-9.&+'-]*$")


def words(t):
    return [w for w in (EDGE.sub("", x) for x in WS.sub(" ", t.lower()).split(" ")) if w]


def hit(text, kw):
    h = WS.sub(" ", text.lower())
    ws = words(kw)
    if not ws:
        return False
    for w in ws:
        if ASCII.match(w):
            if not re.search(r"(?<![a-z0-9])" + re.escape(w) + r"(?![a-z0-9])", h):
                return False
        elif w not in h:
            return False
    return True


topics = conn.execute("SELECT id, name, terms FROM topics WHERE source='auto' AND status='active'").fetchall()

print("\n== 토픽 갱신 입력 줄이기: 최근 6시간 창을 4개(백업 시각에서 거꾸로) ==")
for w in range(4):
    end = now - w * 6 * 3600000
    start = end - 6 * 3600000
    win = [h for h in H if start < h["published_at"] <= end][:1500]
    if len(win) < 20:
        continue
    titles = [h["title"] for h in win]
    jp = [h for h in win if KANA.search(h["title"])]
    en = [h for h in win if not HANGUL.search(h["title"]) and not KANA.search(h["title"])]
    rest = [h for h in win if not KANA.search(h["title"])]
    # 같은 소식 묶기(탐욕: 앞선 묶음의 대표와 비교)
    clusters = []
    for h in rest:
        f = feats(h["title"])
        for c in clusters:
            if same(f, c["f"]):
                c["items"].append(h)
                break
        else:
            clusters.append({"f": f, "items": [h]})
    multi = [c for c in clusters if len({x["publisher"] for x in c["items"]}) >= 2]
    reps_all = [c["items"][0]["title"] for c in clusters]
    reps_multi = [c["items"][0]["title"] for c in multi]
    print(f"창 {w}: 제목 {len(win)}개(약 {est_tokens(titles)/1000:.1f}k토큰) | 일본어 {len(jp)} · 영어 {len(en)} | 일본어 빼고 같은 소식 묶음 {len(clusters)}개(약 {est_tokens(reps_all)/1000:.1f}k) | 언론사 2곳 이상 묶음 {len(multi)}개(약 {est_tokens(reps_multi)/1000:.1f}k)")
    # 진행 중 토픽의 검색어가 줄인 입력에도 남는가
    cover = []
    for t in topics:
        terms = json.loads(t["terms"])
        m_all = sum(1 for x in titles if any(hit(x, k) for k in terms))
        m_rep = sum(1 for x in reps_all if any(hit(x, k) for k in terms))
        m_multi = sum(1 for x in reps_multi if any(hit(x, k) for k in terms))
        cover.append((t["name"], m_all, m_rep, m_multi))
    print("   토픽별 제목 적중(전체/묶음 대표/2곳 이상 묶음):", "; ".join(f"{n[:14]} {a}/{b}/{c}" for n, a, b, c in cover))

print("\n== 피드별 제목 수와 토픽 검색어 적중(보관 3일) ==")
feed = defaultdict(lambda: [0, 0])
allterms = [k for t in topics for k in json.loads(t["terms"])]
for h in H:
    f = feed[h["feed"]]
    f[0] += 1
    f[1] += any(hit(h["title"], k) for k in allterms)
tot = sum(v[0] for v in feed.values())
print("피드 수", len(feed), "| 적중률 낮은 큰 피드(제목 수/적중):")
for k, v in sorted(feed.items(), key=lambda x: (x[1][1] / x[1][0], -x[1][0]))[:14]:
    print(f"   {k}: {v[0]}/{v[1]} ({100*v[1]/v[0]:.1f}%)")
low = [(k, v) for k, v in feed.items() if v[1] / v[0] < 0.02]
print(f"적중률 2% 미만 피드 {len(low)}개가 제목 {sum(v[0] for _, v in low)}개({100*sum(v[0] for _, v in low)/tot:.1f}%)")

print("\n== 종목 별칭: 보관 제목(3일)에서 정식 이름 없이 별칭만 나온 기사 ==")
stocks = {1: ("삼성전자", r"삼전|삼성"), 3: ("현대건설", r"현건(?!설)|힐스테이트|디에이치"), 4: ("두산에너빌리티", r"두산에너빌(?!리티)|두산에너지")}
for sid, (name, pat) in stocks.items():
    rx = re.compile(pat)
    cand = [h for h in H if rx.search(h["title"] + " " + h["summary"]) and name not in (h["title"] + " " + h["summary"])]
    keys = [h["url_key"] for h in cand if h["url_key"]]
    matched = {}
    for i in range(0, len(keys), 500):
        part = keys[i : i + 500]
        q = ",".join("?" * len(part))
        for r in conn.execute(
            f"SELECT a.url_key, e.relevance, e.magnitude FROM articles a JOIN article_matches am ON am.article_id=a.id AND am.stock_id=? LEFT JOIN evaluations e ON e.match_id=am.id WHERE a.url_key IN ({q})",
            (sid, *part),
        ):
            matched[r[0]] = (r[1], r[2])
    passed = sum(1 for v in matched.values() if v[0] is not None and v[0] >= 6 and (v[1] or 0) >= 6)
    title_only = [h for h in cand if rx.search(h["title"])]
    un = [h for h in title_only if h["url_key"] not in matched]
    print(f"{name} (/{pat}/): 별칭만 있는 제목 {len(cand)}개(제목에 별칭 {len(title_only)}) → 키워드로라도 이 종목에 걸린 것 {len(matched)}(통과 {passed}), 제목에 별칭이 있는데 안 걸린 것 {len(un)}")
    for h in un[:12]:
        print("     -", h["title"][:80])
