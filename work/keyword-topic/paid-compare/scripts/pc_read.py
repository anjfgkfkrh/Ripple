import json, glob, sqlite3, sys
W = ".paid-w/paid/res/or/"
conn = sqlite3.connect(".paid-w/db/ripple.db")
def ld(n):
    d = json.load(open(W + n + ".json")); u = d["usage"]
    try: o = json.loads(d["choices"][0]["message"]["content"])
    except Exception: o = None
    return o, u["prompt_tokens"], u["completion_tokens"], u["completion_tokens_details"]["reasoning_tokens"], u["cost"], d["choices"][0]["finish_reason"]
tot = sum(json.load(open(f))["usage"]["cost"] for f in glob.glob(W + "*.json"))
print(f"응답 {len(glob.glob(W+'*.json'))}개 합계 ${tot:.4f}")
print("== 트리 ==")
for sid, nm in ((1,"삼성전자"),(3,"현대건설"),(4,"두산에너빌리티"),(5,"한국 증시"),(109,"미국 증시")):
    for v in ("off", "now", "now2"):
        o, pi, po, pr, c, fr = ld(f"tree_{sid}_{v}")
        if not o: print(nm, v, "읽기 실패"); continue
        kw = o["keywords"]; l1 = [k for k in kw if k["level"]==1]; l2 = [k for k in kw if k["level"]==2]
        l1n = {k["keyword"] for k in l1}
        orphan = sum(k["parent"] not in l1n for k in l2)
        print(f"{nm} {v}: 입력 {pi} 출력 {po} 추론 {pr} ${c:.6f} 통과 O | 1단계 {len(l1)}(≤6 {len(l1)<=6}) 2단계 {len(l2)}(≤12 {len(l2)<=12}) 고아 {orphan} | 별칭 {len(o['aliases'])} 고유어 {len(o['anchors'])} | {fr}")
for n in ("probe_effortnone","probe_effortlow","probe_maxtok1024"):
    o, pi, po, pr, c, fr = ld(n); kw=o["keywords"]
    print(n, f"입력 {pi} 출력 {po} 추론 {pr} ${c:.6f} 1단계 {sum(k['level']==1 for k in kw)} 2단계 {sum(k['level']==2 for k in kw)} 별칭 {len(o['aliases'])} 고유어 {len(o['anchors'])} {fr}")
print("== 토픽 ==")
def pids(o): return [t["id"] for t in o["topics"]]
def sc(o): 
    from collections import Counter
    return Counter(t.get("scope") for t in o["topics"])
for n in ("topic_w0_off","topic_w0_now","topic_w0_now2","topic_w1_off","topic_w1_now","topic_w1_now2"):
    o, pi, po, pr, c, fr = ld(n)
    ids = pids(o); kept = [i for i in ids if i]
    print(n, f"입력 {pi} 출력 {po} 추론 {pr} ${c:.6f} 형식 O 토픽 {len(ids)} 같은id {len(kept)} 새로 {len(ids)-len(kept)} 중복id {len(kept)-len(set(kept))} scope {dict(sc(o))} {fr}")
print("== (b) ==")
for w in (0,1,2):
    info = json.load(open(f".paid-w/paid/info_w{w}.json")); prev = {p["id"]: p for p in info["prev"]}
    for v, n in (("안 보임", f"topic_w{w}_off" if w<2 else "hide_w2_off"), ("보임", f"show_w{w}_off")):
        o, pi, po, pr, c, fr = ld(n)
        ids = pids(o); kept = {i for i in ids if i}
        new = len(ids)-len(kept)
        active = {i for i,p in prev.items() if not p["ended"]}
        dropped = {i for i in active if i not in kept}   # 진행 중이던 것 중 뺀 것
        z = [i for i in active if prev[i]["pass"]==0]; p_ = [i for i in active if prev[i]["pass"]>0]
        revived = [i for i in kept if i in prev and prev[i]["ended"]]
        print(f"창{w} {v}: 입력 {pi} 출력 {po} 추론 {pr} ${c:.6f} | 토픽 {len(ids)} 이어짐 {len(kept)} 새로 {new} 되살림(끝남→) {len(revived)} 뺀(진행중) {len(dropped)} | 통과0 진행중 {len(z)}: 뺀 {sum(i in dropped for i in z)} 남긴 {sum(i not in dropped for i in z)} | 통과>0 진행중 {len(p_)}: 뺀 {sum(i in dropped for i in p_)}")
        print("    뺀 통과0:", [prev[i]["name"] for i in z if i in dropped][:20])
        print("    뺀 통과>0:", [(prev[i]["name"], prev[i]["pass"]) for i in p_ if i in dropped])
