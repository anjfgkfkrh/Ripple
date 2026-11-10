"""M1 요청 만들기: Jev 토큰 구조, 한국·미국 합친 질문, 기사당 한 요청. 문구는 questions.rs와 같게 쓴다."""
import json
import random
import sqlite3
import sys
from collections import defaultdict

DB, OUT = sys.argv[1], sys.argv[2]
MODEL = "jev-1.13.0"
conn = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
conn.row_factory = sqlite3.Row
random.seed(20261110)

TOPIC_REL = [
    "Unrelated to the topic; the keyword match is a coincidence.",
    "Mentions the topic only in passing.",
    "Background or commentary about the topic.",
    "A concrete development in the topic.",
    "An important development in the topic.",
    "The central news of the topic.",
]
MARKET_MAG = [
    "No effect on the overall market.",
    "A negligible effect.",
    "A small or short-lived effect on some sectors.",
    "A noticeable effect on the market or several large sectors for a day or two.",
    "A significant effect that changes market expectations, such as an interest-rate decision or a major policy or geopolitical event.",
    "A major effect likely to move the whole market substantially.",
]
IMPACT = {"positive": "Good news", "negative": "Bad news", "neutral": "No effect, or the direction is unclear"}
MARKET_NAME = {"KR": "Korean stock market (KOSPI, KOSDAQ)", "US": "US stock market (S&P 500, Nasdaq)"}
HOW = "The article mentions a search keyword. path is the chain from the target to the keyword; reason explains how the keyword relates to the previous step."


def score(instr, crit):
    return {"type": "score", "instructions": instr, "criteria": crit}


def choice(instr):
    return {"type": "choice", "instructions": instr, "criteria": IMPACT}


def current_questions():
    return {
        "relevance": score("How directly is the `article` about the topic in `topic`? `link` explains how the article was matched.", TOPIC_REL),
        "magnitude": score("How much could the news in the `article` move the overall stock market in `market`?", MARKET_MAG),
        "impact": choice("What effect does the `article` have on the overall stock market in `market`?"),
    }


def article_state(title, summary):
    a = {"title": title}
    if summary:
        a["summary"] = summary
    return a


def topic_state(name, summary):
    t = {"name": name}
    if summary:
        t["summary"] = summary
    return t


def link_state(path, reason):
    link = {"how": HOW, "path": path}
    if reason:
        link["reason"] = reason
    return link


def current_request(p, market):
    state = {
        "topic": topic_state(p["topic_name"], p["topic_summary"]),
        "market": {"name": MARKET_NAME[market]},
        "link": link_state(p["path"], p["reason"]),
        "article": article_state(p["title"], p["summary"]),
    }
    return {"model": MODEL, "state": state, "questions": current_questions()}


def merged_request(p):
    state = {
        "topic": topic_state(p["topic_name"], p["topic_summary"]),
        "markets": {"kr": {"name": MARKET_NAME["KR"]}, "us": {"name": MARKET_NAME["US"]}},
        "link": link_state(p["path"], p["reason"]),
        "article": article_state(p["title"], p["summary"]),
    }
    q = {"relevance": score("How directly is the `article` about the topic in `topic`? `link` explains how the article was matched.", TOPIC_REL)}
    for m in ("kr", "us"):
        q[f"magnitude_{m}"] = score(f"How much could the news in the `article` move the overall stock market in `markets.{m}`?", MARKET_MAG)
    for m in ("kr", "us"):
        q[f"impact_{m}"] = choice(f"What effect does the `article` have on the overall stock market in `markets.{m}`?")
    return {"model": MODEL, "state": state, "questions": q}


mx = conn.execute("SELECT max(evaluated_at) FROM evaluations").fetchone()[0]
rows = conn.execute(
    """
SELECT e.article_id, s.topic_id, s.market, e.relevance r, e.magnitude m, e.impact, am.keyword_path path, am.keyword_reason reason,
       coalesce(s.name_ko, s.name) topic_name, t.summary topic_summary, a.title, a.summary
FROM evaluations e JOIN article_matches am ON am.id=e.match_id JOIN stocks s ON s.id=e.stock_id
JOIN topics t ON t.id=s.topic_id JOIN articles a ON a.id=e.article_id
WHERE s.kind='topic' AND t.source='auto' AND e.copied_from IS NULL AND e.magnitude IS NOT NULL AND e.evaluated_at >= ?
""",
    (mx - 7 * 86400000,),
).fetchall()
by = defaultdict(dict)
for r in rows:
    by[(r["article_id"], r["topic_id"])][r["market"]] = dict(r)
pairs = [v for v in by.values() if "KR" in v and "US" in v]


def ok(e):
    return e["r"] >= 6 and e["m"] >= 6


strata = {
    "both": [p for p in pairs if ok(p["KR"]) and ok(p["US"])],
    "kr_only": [p for p in pairs if ok(p["KR"]) and not ok(p["US"])],
    "us_only": [p for p in pairs if not ok(p["KR"]) and ok(p["US"])],
    "neither": [p for p in pairs if not ok(p["KR"]) and not ok(p["US"])],
}
want = {"both": 25, "kr_only": 25, "us_only": 15, "neither": 35}
sample = []
for k, n in want.items():
    for p in random.sample(strata[k], min(n, len(strata[k]))):
        sample.append((k, p))
print({k: len(v) for k, v in strata.items()}, "→ 표본", len(sample))

key = []


def put(name, body):
    with open(f"{OUT}/req/jev/{name}.json", "w") as f:
        json.dump(body, f, ensure_ascii=False)


# (a) 토큰 구조: 같은 기사로 질문 수만 바꾼다
p0 = sample[0][1]["KR"]
full = current_request(p0, "KR")
for label, keys in (("a1_rel", ["relevance"]), ("a2_rel_mag", ["relevance", "magnitude"]), ("a3_all", ["relevance", "magnitude", "impact"]), ("a4_mag_imp", ["magnitude", "impact"])):
    body = json.loads(json.dumps(full))
    body["questions"] = {k: full["questions"][k] for k in keys}
    put(label, body)
body = json.loads(json.dumps(full))
body["state"]["article"].pop("summary", None)
put("a5_all_nosummary", body)

# (b) 한국·미국: 지금 형식 다시 묻기(흔들림 기준)와 합친 질문
for i, (stratum, p) in enumerate(sample):
    put(f"b{i:03d}_kr", current_request(p["KR"], "KR"))
    put(f"b{i:03d}_us", current_request(p["US"], "US"))
    put(f"b{i:03d}_mg", merged_request(p["KR"]))
    key.append({"i": i, "stratum": stratum, "kr": {k: p["KR"][k] for k in ("r", "m", "impact")}, "us": {k: p["US"][k] for k in ("r", "m", "impact")}, "title": p["KR"]["title"], "topic": p["KR"]["topic_name"]})

# (c) 기사당 한 요청: 토픽 2~4개에 걸린 기사
arts = defaultdict(list)
for (aid, tid), v in by.items():
    if "KR" in v and "US" in v:
        arts[aid].append(v)
multi = [v for v in arts.values() if 2 <= len(v) <= 4]
ckey = []
for j, group in enumerate(random.sample(multi, min(40, len(multi)))):
    a = group[0]["KR"]
    state = {"topics": {}, "markets": {"kr": {"name": MARKET_NAME["KR"]}, "us": {"name": MARKET_NAME["US"]}}, "article": article_state(a["title"], a["summary"])}
    q = {}
    for n, v in enumerate(group, 1):
        state["topics"][f"t{n}"] = topic_state(v["KR"]["topic_name"], v["KR"]["topic_summary"])
        q[f"relevance_t{n}"] = score(f"How directly is the `article` about the topic in `topics.t{n}`?", TOPIC_REL)
    for m in ("kr", "us"):
        q[f"magnitude_{m}"] = score(f"How much could the news in the `article` move the overall stock market in `markets.{m}`?", MARKET_MAG)
    for m in ("kr", "us"):
        q[f"impact_{m}"] = choice(f"What effect does the `article` have on the overall stock market in `markets.{m}`?")
    put(f"c{j:03d}", {"model": MODEL, "state": state, "questions": q})
    ckey.append({"j": j, "title": a["title"], "topics": [{"name": v["KR"]["topic_name"], "kr": {k: v["KR"][k] for k in ("r", "m", "impact")}, "us": {k: v["US"][k] for k in ("r", "m", "impact")}} for v in group]})
json.dump({"b": key, "c": ckey}, open(f"{OUT}/m1_key.json", "w"), ensure_ascii=False)
import os

files = os.listdir(f"{OUT}/req/jev")
chars = sum(os.path.getsize(f"{OUT}/req/jev/{f}") for f in files)
print("요청 파일", len(files), "개, 합계", chars, "바이트. 예상 입력 약", round(len([f for f in files if f.startswith('b') and not f.endswith('_mg.json')]) * 1019 + 100 * 1400 + 40 * 2000 + 5 * 1000), "토큰 → 약 $", round((200 * 1019 + 100 * 1400 + 40 * 2000 + 5000) * 0.042 / 1e6, 4))
