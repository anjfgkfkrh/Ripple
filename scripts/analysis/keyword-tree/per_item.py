"""항목별 연결·평가·통과(종목·시장, 이름 vs 키워드). 사용: python3 -I per_item.py <db> <시작ms> <끝ms>"""
import sqlite3,sys
from collections import Counter,defaultdict
con=sqlite3.connect(f"file:{sys.argv[1]}?mode=ro",uri=True); con.row_factory=sqlite3.Row
A,B=int(sys.argv[2]),int(sys.argv[3])
q="""select s.name_ko n, m.match_kind mk, count(*) m, sum(m.gated) g, sum(e.copied_from is null and e.id is not null) e,
 sum(e.relevance>=6 and coalesce(e.magnitude,9)>=5) p, sum(e.copied_from is null and e.relevance>=6 and coalesce(e.magnitude,9)>=5) po
 from article_matches m join stocks s on s.id=m.stock_id left join evaluations e on e.match_id=m.id
 where s.kind in ('stock','market') and m.created_at>=? and m.created_at<? group by 1,2 order by 1,2"""
for r in con.execute(q,(A,B)): print(tuple(r))
