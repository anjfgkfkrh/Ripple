import json, math, os, glob
R = "paid/res/jev/"
def load(n): return json.load(open(R + n + ".json"))
def corr(x, y):
    n = len(x); mx = sum(x)/n; my = sum(y)/n
    return sum((a-mx)*(b-my) for a,b in zip(x,y)) / math.sqrt(sum((a-mx)**2 for a in x) * sum((b-my)**2 for b in y))
def mae(x, y): return sum(abs(a-b) for a,b in zip(x,y))/len(x)
tot_in = sum(json.load(open(f))["usage"]["input_tokens"] for f in glob.glob(R + "*.json"))
print(f"응답 {len(glob.glob(R+'*.json'))}개, 입력 토큰 합 {tot_in} → ${tot_in*0.042/1e6:.4f}")
print("\n(a) 토큰 구조(같은 기사):")
for n in ("a1_rel", "a2_rel_mag", "a3_all", "a4_mag_imp", "a5_all_nosummary"):
    d = load(n); print(f"   {n}: 입력 {d['usage']['input_tokens']}, 출력 {d['usage'].get('output_tokens')}", {k: v.get('score', v.get('choice')) for k, v in d['answers'].items()})
key = json.load(open("paid/m1_key.json"))
S = lambda d, q: d["answers"][q]["score"] * 2
B = []
for k in key["b"]:
    i = k["i"]; kr, us, mg = load(f"b{i:03d}_kr"), load(f"b{i:03d}_us"), load(f"b{i:03d}_mg")
    B.append(dict(k=k, kr_r=S(kr,"relevance"), kr_m=S(kr,"magnitude"), us_r=S(us,"relevance"), us_m=S(us,"magnitude"),
                  kr_i=kr["answers"]["impact"]["choice"], us_i=us["answers"]["impact"]["choice"],
                  mg_r=S(mg,"relevance"), mg_mk=S(mg,"magnitude_kr"), mg_mu=S(mg,"magnitude_us"),
                  mg_ik=mg["answers"]["impact_kr"]["choice"], mg_iu=mg["answers"]["impact_us"]["choice"],
                  tok_sep=kr["usage"]["input_tokens"]+us["usage"]["input_tokens"], tok_mg=mg["usage"]["input_tokens"]))
n = len(B)
print(f"\n(b) 한국·미국 합친 질문, 쌍 {n}개")
print(f"   토큰: 따로 2번 평균 {sum(b['tok_sep'] for b in B)/n:.0f} → 합쳐 1번 평균 {sum(b['tok_mg'] for b in B)/n:.0f} ({100*(1-sum(b['tok_mg'] for b in B)/sum(b['tok_sep'] for b in B)):.0f}% 절감)")
st = lambda m, f: [b["k"][m][f] for b in B]
print("   흔들림 기준(저장된 점수 vs 지금 형식으로 다시 물은 점수): 평균 차이 관련도 KR %.2f, 영향도 KR %.2f, 영향도 US %.2f" % (mae(st("kr","r"), [b["kr_r"] for b in B]), mae(st("kr","m"), [b["kr_m"] for b in B]), mae(st("us","m"), [b["us_m"] for b in B])))
print("   합친 질문 vs 다시 물은 점수: 평균 차이 관련도 %.2f(상관 %.3f), 영향도 KR %.2f(상관 %.3f), 영향도 US %.2f(상관 %.3f)" % (
    mae([b["kr_r"] for b in B],[b["mg_r"] for b in B]), corr([b["kr_r"] for b in B],[b["mg_r"] for b in B]),
    mae([b["kr_m"] for b in B],[b["mg_mk"] for b in B]), corr([b["kr_m"] for b in B],[b["mg_mk"] for b in B]),
    mae([b["us_m"] for b in B],[b["mg_mu"] for b in B]), corr([b["us_m"] for b in B],[b["mg_mu"] for b in B])))
print("   치우침(합친 - 다시 물은): 관련도 %+.2f, 영향도 KR %+.2f, 영향도 US %+.2f" % (
    sum(b["mg_r"]-b["kr_r"] for b in B)/n, sum(b["mg_mk"]-b["kr_m"] for b in B)/n, sum(b["mg_mu"]-b["us_m"] for b in B)/n))
ok = lambda r, m: r >= 6 and m >= 6
for mk, r1, m1, m2 in (("KR", "kr_r", "kr_m", "mg_mk"), ("US", "us_r", "us_m", "mg_mu")):
    a = [ok(b[r1], b[m1]) for b in B]; c = [ok(b["mg_r"], b[m2]) for b in B]
    s = [ok(b["k"][mk.lower()]["r"], b["k"][mk.lower()]["m"]) for b in B]
    print(f"   {mk} 통과 판정: 저장 {sum(s)} / 다시 {sum(a)} / 합침 {sum(c)}. 다시 vs 합침 일치 {sum(x==y for x,y in zip(a,c))}/{n} (다시만 통과 {sum(x and not y for x,y in zip(a,c))}, 합침만 통과 {sum(y and not x for x,y in zip(a,c))}). 저장 vs 다시 일치 {sum(x==y for x,y in zip(s,a))}/{n}")
print(f"   호재/악재 일치(다시 vs 합침): KR {sum(b['kr_i']==b['mg_ik'] for b in B)}/{n}, US {sum(b['us_i']==b['mg_iu'] for b in B)}/{n}; 다시 물은 KR vs 저장 {sum(b['kr_i']==b['k']['kr']['impact'] for b in B)}/{n}")
print(f"   따로 물은 관련도 KR vs US 평균 차이 {mae([b['kr_r'] for b in B],[b['us_r'] for b in B]):.2f}")
print("\n(c) 기사당 한 요청(토픽 2~4개)")
rs, rm, toks, sep, mk_s, mk_m, mu_s, mu_m, flips = [], [], 0, 0, [], [], [], [], 0
pk = pm = 0; tn = 0
for c in key["c"]:
    d = load(f"c{c['j']:03d}"); toks += d["usage"]["input_tokens"]; sep += len(c["topics"]) * 2 * 1019
    mk, mu = S(d, "magnitude_kr"), S(d, "magnitude_us")
    for t_i, t in enumerate(c["topics"], 1):
        r = S(d, f"relevance_t{t_i}"); rs.append(t["kr"]["r"]); rm.append(r); tn += 1
        mk_s.append(t["kr"]["m"]); mk_m.append(mk); mu_s.append(t["us"]["m"]); mu_m.append(mu)
        a, b = ok(t["kr"]["r"], t["kr"]["m"]), ok(r, mk); pk += a == b
        a, b = ok(t["us"]["r"], t["us"]["m"]), ok(r, mu); pm += a == b
print(f"   기사 {len(key['c'])}건·토픽 연결 {tn}개. 토큰: 지금 방식 약 {sep} → 한 요청 {toks} ({100*(1-toks/sep):.0f}% 절감, 요청당 평균 {toks/len(key['c']):.0f})")
print(f"   저장 점수 대비: 관련도 평균 차이 {mae(rs, rm):.2f}(상관 {corr(rs, rm):.3f}), 영향도 KR {mae(mk_s, mk_m):.2f}(상관 {corr(mk_s, mk_m):.3f}), US {mae(mu_s, mu_m):.2f}(상관 {corr(mu_s, mu_m):.3f})")
print(f"   통과 판정 일치(저장 vs 한 요청): KR {pk}/{tn}, US {pm}/{tn}")
