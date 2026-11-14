"""measure 유료 측정 결과 읽기. 사용: python3 -I measure_read.py <paid 폴더> > consistency.txt
paid/sample.json과 paid/res/jev/*.json을 읽는다. 확률은 응답의 noul 값(예일 확률). 점수는 응답 score를 2배로 해 저장값(0~10)과 맞춘다.
"""
import json
import sys
from collections import Counter, defaultdict
from pathlib import Path

P = Path(sys.argv[1])
sample = json.loads((P / "sample.json").read_text())
R = lambda n, kind, r: json.loads((P / f"res/jev/{n}_{kind}_r{r}.json").read_text())
NAT = ["fact", "analysis", "forecast", "opinion", "media", "roundup"]
GROUPS = ["①", "④", "회사", "형식"]
mean = lambda xs: sum(xs) / len(xs)

tot_in = sum(json.loads(f.read_text())["usage"]["input_tokens"] for f in (P / "res/jev").glob("*.json"))
n_res = len(list((P / "res/jev").glob("*.json")))
print(f"응답 {n_res}개, 입력 토큰 합 {tot_in} → ${tot_in * 0.042 / 1e6:.4f} (출력 토큰 요금은 계산하지 않음, 합 {sum(json.loads(f.read_text())['usage'].get('output_tokens', 0) for f in (P / 'res/jev').glob('*.json'))})")
for kind in ("nature", "stock"):
    t = [R(e["n"], kind, r)["usage"]["input_tokens"] for e in sample for r in (1, 2, 3)]
    print(f"{kind} 요청 평균 입력 토큰 {mean(t):.0f} (최소 {min(t)}, 최대 {max(t)}, {len(t)}회)")

by = defaultdict(list)
for e in sample:
    by[e["group"]].append(e)
print("묶음별 기사 수:", {g: len(by[g]) for g in GROUPS})


def vals(e, kind, q):
    return [R(e["n"], kind, r)["answers"][q]["noul"] for r in (1, 2, 3)]


print("\n## 질문별 일관성 (묶음별. 같은 쪽 = 3번 모두 0.5 이상이거나 모두 0.5 미만. 확률은 기사×3번 값 전체의 평균/최소/최대, 흔들림 = 기사별 (최대-최소)의 평균)")
print("| 질문 | 묶음 | 같은 쪽 | 평균 | 최소 | 최대 | 흔들림 | 0.5 이상 비율(호출 단위) |\n|---|---|---|---|---|---|---|---|")
for kind, qs in (("nature", NAT), ("stock", ["bystander"])):
    for q in qs:
        for g in GROUPS:
            vs = [vals(e, kind, q) for e in by[g]]
            flat = [v for x in vs for v in x]
            same = sum(all(v >= 0.5 for v in x) or all(v < 0.5 for v in x) for x in vs)
            print(f"| {q} | {g} | {same}/{len(vs)} ({100*same/len(vs):.0f}%) | {mean(flat):.2f} | {min(flat):.2f} | {max(flat):.2f} | {mean([max(x)-min(x) for x in vs]):.2f} | {100*sum(v >= 0.5 for v in flat)/len(flat):.0f}% |")

print("\n## bystander 0.5 이상 비율 (묶음별, 호출 단위 / 기사 3번 평균 기준)")
print("| 묶음 | 호출 단위 | 3번 평균 기준 |\n|---|---|---|")
for g in GROUPS:
    vs = [vals(e, "stock", "bystander") for e in by[g]]
    print(f"| {g} | {sum(v >= 0.5 for x in vs for v in x)}/{3*len(vs)} | {sum(mean(x) >= 0.5 for x in vs)}/{len(vs)} |")


def tags(a):
    p = {q: a[q]["noul"] >= 0.5 for q in NAT}
    if p["media"] or p["roundup"]:
        return ("숨김",)
    t = [n for n, q in (("사실", "fact"), ("해석", "analysis"), ("전망", "forecast")) if p[q]]
    if not t or p["opinion"]:
        t.append("기타")
    return tuple(t)


print("\n## 성격 태그 (사실·해석·전망, 셋 다 아니거나 opinion이면 기타, media·roundup이면 숨김)")
print("### 태그 조합 분포 (호출 단위, 기사×3번)")
combos = sorted({tags(R(e["n"], "nature", r)["answers"]) for e in sample for r in (1, 2, 3)}, key=lambda c: (len(c), c))
print("| 조합 | " + " | ".join(GROUPS) + " |\n|---|" + "---|" * len(GROUPS))
for c in combos:
    row = []
    for g in GROUPS:
        cnt = sum(tags(R(e["n"], "nature", r)["answers"]) == c for e in by[g] for r in (1, 2, 3))
        row.append(f"{cnt} ({100*cnt/(3*len(by[g])):.0f}%)")
    print(f"| {'+'.join(c)} | " + " | ".join(row) + " |")
print("\n### 3번 모두 같은 태그 조합인 기사 비율")
print("| 묶음 | 같은 조합 |\n|---|---|")
tot = 0
for g in GROUPS:
    s = sum(len({tags(R(e["n"], "nature", r)["answers"]) for r in (1, 2, 3)}) == 1 for e in by[g])
    tot += s
    print(f"| {g} | {s}/{len(by[g])} ({100*s/len(by[g]):.0f}%) |")
print(f"| 전체 | {tot}/{len(sample)} ({100*tot/len(sample):.0f}%) |")
print("\n### 3번 조합이 갈린 기사")
for g in GROUPS:
    for e in by[g]:
        cs = [tags(R(e["n"], "nature", r)["answers"]) for r in (1, 2, 3)]
        if len(set(cs)) > 1:
            print(f"- {e['n']} [{g}] " + " / ".join("+".join(c) for c in cs) + f" — {e['title'][:60]}")

print("\n## 종목 요청의 관련도·영향도 vs 저장값 (점수×2, 3번 평균. 평균 차이 = |새-저장| 평균, 부호 = 새-저장 평균)")
print("| 묶음 | 관련도 평균 차이 | 관련도 부호 | 영향도(크기) 평균 차이 | 영향도(크기) 부호 | 호재/악재 일치 | 관련도 3번 흔들림 | 크기 3번 흔들림 |\n|---|---|---|---|---|---|---|---|")
for g in GROUPS + ["전체"]:
    es = sample if g == "전체" else by[g]
    dr, dm, ok, wr, wm, nm = [], [], 0, [], [], 0
    for e in es:
        rs = [R(e["n"], "stock", r)["answers"] for r in (1, 2, 3)]
        r_ = [a["relevance"]["score"] * 2 for a in rs]
        m_ = [a["magnitude"]["score"] * 2 for a in rs]
        dr.append(mean(r_) - e["relevance"])
        wr.append(max(r_) - min(r_)); wm.append(max(m_) - min(m_))
        if e["magnitude"] is not None:
            dm.append(mean(m_) - e["magnitude"])
        ok += sum(a["impact"]["choice"] == e["impact"] for a in rs)
        nm += 3
    print(f"| {g} | {mean([abs(x) for x in dr]):.2f} | {mean(dr):+.2f} | {mean([abs(x) for x in dm]):.2f} | {mean(dm):+.2f} | {ok}/{nm} | {mean(wr):.2f} | {mean(wm):.2f} |")

print("\n## 표본별 한 줄 (3번 평균 확률 f/a/fc/o/m/r, bystander, 태그, 새 관련도·크기(저장))")
for e in sample:
    nv = {q: mean(vals(e, "nature", q)) for q in NAT}
    by_v = vals(e, "stock", "bystander")
    rs = [R(e["n"], "stock", r)["answers"] for r in (1, 2, 3)]
    print(f"{e['n']} [{e['group']}{'/' + e['tag'] if e['tag'] else ''}] " + "/".join(f"{nv[q]:.2f}" for q in NAT)
          + f" by={'/'.join(f'{v:.2f}' for v in by_v)} rel={mean([a['relevance']['score']*2 for a in rs]):.1f}({e['relevance']:.1f}) mag={mean([a['magnitude']['score']*2 for a in rs]):.1f}({e['magnitude'] if e['magnitude'] is None else round(e['magnitude'],1)}) {e['label']} {e['title'][:40]}")
