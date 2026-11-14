"""measure 유료 측정 요청 만들기. 예/아니오 질문 type은 SDK 0.7.0 기준 "noul"(응답 noul=예 확률, "boolean"은 HTTP 400). 사용: python3 -I measure_make.py <DB 사본> <paid 폴더> [first]
paid/sample.json(표본 50건과 저장된 값), paid/req/jev/*.json(성격 요청·종목 요청, 3번씩)을 쓴다. first면 첫 요청 1개만.
표본 시드는 고정(20261114). 화면 기준·표본 ①·④ 맞추기는 measure_free.py와 같다.
"""
import json
import random
import re
import sqlite3
import sys
from pathlib import Path

DB, OUT = sys.argv[1], Path(sys.argv[2])
FIRST = len(sys.argv) > 3 and sys.argv[3] == "first"
HERE = Path(__file__).resolve().parent
FIXTURE = HERE.parents[2] / "rust/ripple/src/server/ai/decider/questions_fixture.json"
MODEL = "jev-1.13.0"
rng = random.Random(20261114)
db = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)

NATURE = {
    "fact": "Does the `article` report facts: events, announcements, results, or market moves that have already happened?",
    "analysis": "Does the `article` interpret the causes, meaning, or background of the news, beyond restating the facts?",
    "forecast": "Does the `article` present a prediction or outlook, such as a target price, an earnings estimate, or an expected market direction?",
    "opinion": "Is the `article` an editorial, column, or opinion piece arguing the writer's position?",
    "media": "Is the `article` mainly a photo or video, with only a caption or short description?",
    "roundup": "Is the `article` a roundup that lists several unrelated news items?",
}
BYSTANDER = "Does the `article` mainly report how something else (a stock index, another company, or an institution) is affected, with the company in `stock` appearing only as a cause or in passing?"
LINK_HOW = {
    "name": "The article mentions the company name.",
    "direct": "The news provider tagged this article as news about this company.",
    "keyword": "The article mentions a search keyword. path is the chain from the target to the keyword; reason explains how the keyword relates to the previous step.",
}


def setting(key, default):
    r = db.execute("SELECT value FROM settings WHERE key=?", (key,)).fetchone()
    return float(r[0]) if r else default


MINREL, MINMAG = setting("evaluation.minRelevance", 6.0), setting("evaluation.minMagnitude", 5.0)
cols = """e.id, a.id, a.title, a.summary, s.name, s.name_ko, s.market, s.exchange, s.code, s.aliases,
          m.match_kind, m.keyword_path, m.keyword_reason, e.relevance, e.magnitude, e.impact"""
rows = db.execute(
    f"""SELECT {cols} FROM evaluations e JOIN stocks s ON s.id=e.stock_id JOIN articles a ON a.id=e.article_id
        JOIN article_matches m ON m.id=e.match_id WHERE s.kind='stock' AND s.deleted_at IS NULL"""
).fetchall()
K = ["eval_id", "article_id", "title", "summary", "name", "name_ko", "market", "exchange", "code", "aliases",
     "match_kind", "path", "reason", "relevance", "magnitude", "impact"]
evals = [dict(zip(K, r)) for r in rows]
for e in evals:
    e["shown"] = e["relevance"] >= MINREL and (e["magnitude"] is None or e["magnitude"] >= MINMAG)
    e["label"] = e["name_ko"] or e["name"]

labeled = {}
for line in (HERE / "labeled-2026-11-13.txt").read_text().splitlines():
    if line and not line.startswith("##"):
        p = line.split(" ", 6)
        labeled[(p[1], p[6].strip())] = p[0]
key = lambda e: (e["label"], e["title"])
used = set()


def take(cands, n, group, tag=None):
    cands = list(cands)
    rng.shuffle(cands)
    out = []
    for e in cands:
        if e["article_id"] in used:
            continue
        used.add(e["article_id"])
        out.append(dict(e, group=group, tag=tag))
        if len(out) == n:
            break
    return out


name_shown = [e for e in evals if e["shown"] and e["match_kind"] == "name"]
s1 = take([e for e in name_shown if labeled.get(key(e)) == "①"], 15, "①")
s4 = take([e for e in name_shown if labeled.get(key(e)) == "④"], 10, "④")


def title_has_name(e):
    names = [e["name"], e["name_ko"], *json.loads(e["aliases"] or "[]")]
    t = e["title"].lower().replace(" ", "")
    return any(n and n.lower().replace(" ", "") in t for n in names)


FORMAT = re.compile(r"^[\s\[【(]*(포토|사진|자막뉴스|영상|사설|기자수첩|[^\]】)\s]*칼럼|기고)")
co = [e for e in name_shown if key(e) not in labeled and title_has_name(e) and not FORMAT.search(e["title"])]
s_co = take(co, 15, "회사")

# 형식 기사: 화면 기준을 넘는 것을 먼저, 모자라면 이름 연결 중 관련도 높은 것으로 채운다(선택 근거는 tag에 남김)
PAT = {
    "photo": r"^[\s\[【(]*(포토|사진|자막뉴스)[\]】)]",
    "roundup": r"(\[[^\]]*(브리핑|이슈 ?종합|이슈픽|모닝|조간)[^\]]*\]|\s外\s|\s외\s*$)",
    "sasul": r"^[\s\[【(]*사설[\]】)]",
    "desk": r"^[\s\[【(]*기자수첩[\]】)]",
    "column": r"^[\s\[【(]*[^\]】)\s]*칼럼[^\]】)]*[\]】)]",
}


def fmt(pat, n, tag):
    allc = [e for e in evals if re.search(PAT[pat], e["title"]) and e["article_id"] not in used and e["match_kind"] == "name"]
    # 같은 기사에 여러 종목이면 관련도 가장 큰 것
    best = {}
    for e in allc:
        if e["article_id"] not in best or e["relevance"] > best[e["article_id"]]["relevance"]:
            best[e["article_id"]] = e
    pool = list(best.values())
    got = take([e for e in pool if e["shown"]], n, "형식", tag)
    if len(got) < n:
        got += take(sorted(pool, key=lambda e: -e["relevance"]), n - len(got), "형식", tag + "(화면 기준 미달)")
    return got


s_fm = fmt("photo", 4, "포토·자막뉴스") + fmt("roundup", 3, "뉴스 모음") + fmt("sasul", 1, "사설") + fmt("desk", 1, "기자수첩") + fmt("column", 1, "칼럼")
sample = s1 + s4 + s_co + s_fm
for i, e in enumerate(sample, 1):
    e["n"] = f"s{i:02d}"
OUT.mkdir(parents=True, exist_ok=True)
(OUT / "sample.json").write_text(json.dumps(sample, ensure_ascii=False, indent=1))
print({g: sum(e["group"] == g for e in sample) for g in ("①", "④", "회사", "형식")}, "합", len(sample))

fx = json.loads(FIXTURE.read_text())["articleQuestions"]["stock"]
stock_q = {**fx, "bystander": {"type": "noul", "instructions": BYSTANDER}}
nature_q = {k: {"type": "noul", "instructions": v} for k, v in NATURE.items()}


def article(e):
    a = {"title": e["title"]}
    if e["summary"]:
        a["summary"] = e["summary"]
    return a


def stock_state(e):
    st = {"name": e["label"]}
    if e["name_ko"] and e["name_ko"] != e["name"]:
        st["official_name"] = e["name"]
    st.update(market="Korea" if e["market"] == "KR" else "United States", exchange=e["exchange"], code=e["code"])
    link = {"how": LINK_HOW[e["match_kind"]], "path": e["path"]}
    if e["reason"]:
        link["reason"] = e["reason"]
    return {"stock": st, "link": link, "article": article(e)}


for d in ("req/jev", "res/jev"):
    (OUT / d).mkdir(parents=True, exist_ok=True)
count = 0
for e in sample:
    for r in (1, 2, 3):
        for kind, body in (("nature", {"model": MODEL, "state": {"article": article(e)}, "questions": nature_q}),
                           ("stock", {"model": MODEL, "state": stock_state(e), "questions": stock_q})):
            (OUT / f"req/jev/{e['n']}_{kind}_r{r}.json").write_text(json.dumps(body, ensure_ascii=False))
            count += 1
            if FIRST:
                sys.exit(print("first 1개:", f"{e['n']}_{kind}_r{r}.json") or 0)
print("요청", count)
