"""사실 기사 정의 측정(2026-11-14). 사용: python3 -I fact_measure.py make|read <표본 sample.json> <paid 폴더> [정답 labels.txt]
make: 표본 50건(measure 단계 sample.json)에 성격 요청을 안마다 3번씩 paid/req/jev/<안>_<n>_r<k>.json으로 쓴다.
read: paid/res/jev 응답을 3번 평균해 안마다 사실 탭(사실 ≥ 0.5, 해석·전망 < 0.5)·사실 태그를 정답(Y/N, ?는 뺌)과 비교한다.
old는 measure 단계 응답(archive measure/data/res/jev/<n>_nature_r<k>.json)을 쓴다.
"""
import json
import statistics
import sys
from pathlib import Path

MODE, SAMPLE, OUT = sys.argv[1], Path(sys.argv[2]), Path(sys.argv[3])
MODEL = "jev-1.13.0"
T = 0.5
OLD = {
    "fact": "Does the `article` report facts: events, announcements, results, or market moves that have already happened?",
    "analysis": "Does the `article` interpret the causes, meaning, or background of the news, beyond restating the facts?",
    "forecast": "Does the `article` present a prediction or outlook, such as a target price, an earnings estimate, or an expected market direction?",
    "opinion": "Is the `article` an editorial, column, or opinion piece arguing the writer's position?",
    "media": "Is the `article` mainly a photo or video, with only a caption or short description?",
    "roundup": "Is the `article` a roundup that lists several unrelated news items?",
}
NEW_FACT = (
    "Is the `article` mainly a report of something that has just happened, such as an event, announcement, "
    "result, or market move, rather than an article centered on explaining its causes or meaning or on "
    "predicting what comes next?"
)
VARIANTS = {
    # v1: 결정 1A 그대로(사실 문구만 바꿈)
    "v1": {**OLD, "fact": NEW_FACT},
    # v2: 비교용, 해석·전망도 "주로"로 좁힘
    "v2": {
        **OLD,
        "fact": NEW_FACT,
        "analysis": "Is the `article` mainly an interpretation of the causes, meaning, or background of the news, rather than a report of what happened?",
        "forecast": "Is the `article` mainly about a prediction or outlook, such as a target price, an earnings estimate, or an expected market direction?",
    },
}
sample = json.loads(SAMPLE.read_text())


def article(e):
    a = {"title": e["title"]}
    if e["summary"]:
        a["summary"] = e["summary"]
    return a


if MODE == "make":
    (OUT / "req/jev").mkdir(parents=True, exist_ok=True)
    (OUT / "res/jev").mkdir(parents=True, exist_ok=True)
    count = 0
    for v, qs in VARIANTS.items():
        questions = {k: {"type": "noul", "instructions": q} for k, q in qs.items()}
        for e in sample:
            for r in (1, 2, 3):
                body = {"model": MODEL, "state": {"article": article(e)}, "questions": questions}
                (OUT / f"req/jev/{v}_{e['n']}_r{r}.json").write_text(json.dumps(body, ensure_ascii=False))
                count += 1
    sys.exit(print("요청", count) or 0)

labels = {}
for line in Path(sys.argv[4]).read_text().splitlines():
    if line and not line.startswith("#"):
        n, lab = line.split()[:2]
        labels[n] = lab
old_res = SAMPLE.parent / "res/jev"


def probs(v, n):
    files = sorted(old_res.glob(f"{n}_nature_r*.json")) if v == "old" else sorted((OUT / "res/jev").glob(f"{v}_{n}_r*.json"))
    runs = [json.loads(f.read_text())["answers"] for f in files]
    if not runs:
        return None, []
    keys = runs[0].keys()
    return {k: statistics.mean(r[k]["noul"] for r in runs) for k in keys}, runs


def score(name, pred):
    tp = sum(1 for n, lab in labels.items() if lab == "Y" and pred[n])
    fp = sum(1 for n, lab in labels.items() if lab == "N" and pred[n])
    fn = sum(1 for n, lab in labels.items() if lab == "Y" and not pred[n])
    amb = sum(1 for n, lab in labels.items() if lab == "?" and pred[n])
    print(f"{name:14} 사실로 봄 {sum(pred.values()):2}/50 | 정답 Y 맞힘 {tp}/{tp + fn} | N인데 사실 {fp} | 애매 중 사실 {amb}")


rows = {}
for v in ("old", *VARIANTS):
    rows[v] = {e["n"]: probs(v, e["n"]) for e in sample}
    if any(p is None for p, _ in rows[v].values()):
        print(v, "응답 없음")
        continue
    m = {n: p for n, (p, _) in rows[v].items()}
    score(f"{v} 사실 태그", {n: p["fact"] >= T for n, p in m.items()})
    score(f"{v} 사실 탭(2A)", {n: p["fact"] >= T and p["analysis"] < T and p["forecast"] < T for n, p in m.items()})
    spread = [max(r["fact"]["noul"] for r in runs) - min(r["fact"]["noul"] for r in runs) for _, runs in rows[v].values()]
    print(f"{'':14} 사실 확률 3번 흔들림 평균 {statistics.mean(spread):.3f} 최대 {max(spread):.2f}")

print("\n기사별(정답 · 안마다 사실/해석/전망 평균, 사실 탭이면 *)")
for e in sample:
    cells = []
    for v in rows:
        p = rows[v][e["n"]][0]
        if p:
            tab = "*" if p["fact"] >= T and p["analysis"] < T and p["forecast"] < T else " "
            cells.append(f"{v} {p['fact']:.2f}/{p['analysis']:.2f}/{p['forecast']:.2f}{tab}")
    print(e["n"], labels.get(e["n"], "-"), " | ".join(cells), "|", e["title"][:40])
