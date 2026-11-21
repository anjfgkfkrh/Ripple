"""이슈 낱말별로 수집 기사가 해당 항목에 걸렸는지. 사용: python3 -I issues.py <db> <시작ms>"""
import sqlite3,sys
from collections import Counter
con=sqlite3.connect(f"file:{sys.argv[1]}?mode=ro",uri=True); con.row_factory=sqlite3.Row
A=int(sys.argv[2])
name={r['id']:(r['name_ko'] or r['name']) for r in con.execute("select * from stocks")}
ISSUES=[ # (항목 id, 이슈, 제목 낱말들(하나라도))
 (1,'주주환원·특별배당',['특별배당','주주환원','자사주']),(1,'노조·성과급',['노조','성과급']),(1,'데이터센터 규제·블루웨이브',['블루웨이브','데이터센터 규제']),
 (1,'HBM',['HBM']),(1,'외국인 매도',['외국인']),(1,'목표가',['목표가','목표주가']),
 (2,'실적 발표',['실적','어닝','가이던스']),(2,'젠슨 황',['젠슨 황','젠슨황']),(2,'중국 수출',['수출 통제','수출 규제','대중','H20']),(2,'블루웨이브·데이터센터',['블루웨이브','데이터센터']),
 (3,'원전',['원전']),(3,'도시정비·재건축',['정비사업','재건축','재개발','도시정비']),(3,'원가율',['원가']),(3,'주택 분양',['분양']),
 (4,'SMR',['SMR','소형모듈','뉴스케일','엑스에너지']),(4,'가스터빈',['가스터빈']),(4,'체코 원전',['체코','두코바니']),(4,'AI 전력',['AI 전력','전력 수요','데이터센터 전력']),
 (5,'외국인 순매도',['외국인']),(5,'환율',['환율','원·달러','원달러']),(5,'유가',['유가']),(5,'한은 금리',['금통위','한은','한국은행','기준금리']),(5,'정부 자본시장',['자본시장','밸류업','상법']),
 (109,'연준 금리 인상',['연준','FOMC','금리 인상','빅스텝','Fed']),(109,'PPI·CPI',['PPI','CPI','물가']),(109,'중간선거·블루웨이브',['중간선거','블루웨이브','민주당']),(109,'AI 거품·기술주',['AI 거품','AI 버블','기술주']),(109,'유가',['유가']),
]
links={}
for r in con.execute("""select m.article_id aid,m.stock_id sid,m.match_kind mk,m.gated g,e.relevance rel,e.magnitude mag from article_matches m left join evaluations e on e.match_id=m.id where m.created_at>=?""",(A,)):
    links[(r['aid'],r['sid'])]=r
arts=con.execute("select id,title from articles where collected_at>=?",(A,)).fetchall()
print("항목\t이슈\t제목에 낱말\t걸림(이름/키워드/gated)\t통과\t안 걸림 예")
for sid,iss,words in ISSUES:
    hit=[a for a in arts if any(w.lower() in a['title'].lower() for w in words)]
    c=Counter(); miss=[]
    for a in hit:
        l=links.get((a['id'],sid))
        if not l: miss.append(a['title']); continue
        c['g' if l['g'] else l['mk']]+=1
        if l['rel'] is not None and l['rel']>=6 and (l['mag'] is None or l['mag']>=5): c['p']+=1
    print(f"{name[sid]}\t{iss}\t{len(hit)}\t{c['name']+c['direct']}/{c['keyword']}/{c['g']}\t{c['p']}\t{' | '.join(t[:45] for t in miss[:4])}")
