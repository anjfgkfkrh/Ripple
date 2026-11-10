import json, glob, os, sys
R = "paid/res/or/"
info = json.load(open("paid/m23_info.json"))
tot = 0
def load(n):
    d = json.load(open(R + n + ".json")); u = d["usage"]
    det = u.get("completion_tokens_details") or {}
    try: out = json.loads(d["choices"][0]["message"]["content"])
    except Exception as e: out = None
    return out, u["prompt_tokens"], u["completion_tokens"], det.get("reasoning_tokens"), u.get("cost", 0), d["choices"][0].get("finish_reason")
for f in sorted(glob.glob(R + "*.json")): tot += json.load(open(f))["usage"].get("cost", 0)
print(f"OpenRouter 응답 {len(glob.glob(R+'*.json'))}개, 청구 합계 ${tot:.4f}")
print("\n== M2 트리 ==")
for sid, it in info["items"].items():
    o, pi, po, pr, c, fr = load(f"m2_v0d2_{sid}")
    n0 = len(o["keywords"]) if o else None
    lv = {l: sum(k["level"] == l for k in o["keywords"]) for l in (1, 2, 3)} if o else None
    print(f"[{it['name']}] 지금 프롬프트·깊이 2: 입력 {pi}, 출력 {po}(추론 {pr}), ${c:.4f}, 키워드 {n0}개 {lv}, {fr}")
    for r in (1, 2):
        o, pi, po, pr, c, fr = load(f"m2_v1_{sid}_r{r}")
        if not o: print("   새 형식 r%d: 읽기 실패 %s" % (r, fr)); continue
        l1 = [k["keyword"] for k in o["keywords"] if k["level"] == 1]; l2 = [k["keyword"] for k in o["keywords"] if k["level"] == 2]
        print(f"   새 형식 r{r}: 입력 {pi}, 출력 {po}(추론 {pr}), ${c:.4f}, 1단계 {len(l1)}·2단계 {len(l2)}, {fr}")
        print(f"      별칭 {o['aliases']} | 고유어 {o['anchors']}")
        print(f"      1단계 {l1}")
        print(f"      2단계 {l2}")
    a = load(f"m2_v1_{sid}_r1")[0]; b = load(f"m2_v1_{sid}_r2")[0]
    if a and b:
        ka, kb = {k["keyword"] for k in a["keywords"]}, {k["keyword"] for k in b["keywords"]}
        print(f"   두 번 겹침: 키워드 {len(ka & kb)}/{len(ka | kb)}, 고유어 {len(set(a['anchors']) & set(b['anchors']))}/{len(set(a['anchors']) | set(b['anchors']))}")
