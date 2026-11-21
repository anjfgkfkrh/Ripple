"""경로별 통과율(복사 제외)·18단계 대체 숨김·비용·17단계 전 비교. 사용: python3 -I rates.py <db> <시작ms> <끝ms> <최소영향도>"""
import sqlite3,sys,random
from collections import Counter,defaultdict
con=sqlite3.connect(f"file:{sys.argv[1]}?mode=ro",uri=True); con.row_factory=sqlite3.Row
A,B,MAG=int(sys.argv[2]),int(sys.argv[3]),float(sys.argv[4])
stocks={r["id"]:r for r in con.execute("select * from stocks")}
rows=con.execute("""select m.id mid,m.article_id aid,m.stock_id sid,m.match_kind mk,m.level lv,m.gated g,m.keyword_path path,
 e.relevance rel,e.magnitude mag,e.copied_from cp,a.title from article_matches m join articles a on a.id=m.article_id
 left join evaluations e on e.match_id=m.id where m.created_at>=? and m.created_at<?""",(A,B)).fetchall()
# 같은 기사 시장 평가 최고 관련도(18단계 숨김 대체 규칙: 종목 평가보다 시장 관련도가 높으면 숨김)
mrel=defaultdict(float)
for r in con.execute("select e.article_id,e.relevance from evaluations e join stocks s on s.id=e.stock_id where s.kind='market' and s.deleted_at is null"):
    mrel[r[0]]=max(mrel[r[0]],r[1])
def P(r): return r["rel"] is not None and r["rel"]>=6 and (r["mag"] is None or r["mag"]>=MAG)
topic_paths=set()
def cls(r):
    s=stocks[r["sid"]]
    if s["kind"]=="topic": return "auto-topic"
    if r["mk"]!="keyword": return s["kind"]+":"+r["mk"]
    return s["kind"]+":kw"+str(min(r["lv"],2))
c=defaultdict(Counter)
for r in rows:
    k=cls(r); x=c[k]; x["m"]+=1; x["g"]+=r["g"]
    if r["rel"] is None: continue
    orig=r["cp"] is None
    x["e"]+=orig; x["c"]+=not orig
    if P(r):
        x["p"]+=1; x["po"]+=orig
        if stocks[r["sid"]]["kind"]=="stock" and mrel.get(r["aid"],0)>r["rel"]: x["hid"]+=1
print(f"{'경로':14s} 연결 gated 평가(원) 복사 통과(전체) 통과(원) 통과율(원) 18단계숨김")
for k in sorted(c):
    x=c[k]; print(f"{k:14s} {x['m']:5d} {x['g']:5d} {x['e']:6d} {x['c']:5d} {x['p']:6d} {x['po']:6d} {x['po']/max(x['e'],1):7.1%} {x['hid']:5d}")
# 비용: usage_logs article_evaluation 평균 단가
u=con.execute("select count(*),sum(cost_usd) from usage_logs where purpose='article_evaluation' and at>=? and at<?",(A,B)).fetchone()
tot=con.execute("select sum(cost_usd) from usage_logs where at>=? and at<?",(A,B)).fetchone()[0]
print("평가 사용량 행",u[0],"평가 원본 합",sum(x['e'] for x in c.values()),"단가",(u[1] or 0)/max(u[0],1),"창 전체 비용",tot)
for r in con.execute("select purpose,count(*),round(sum(cost_usd),4) from usage_logs where at>=? and at<? group by 1",(A,B)): print("  ",tuple(r))
