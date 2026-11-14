"""종목 이름으로 걸려 화면에 보이는 연결을 classify.py 입력(TSV)으로 뽑는다.

사용: python3 -I export_shown.py <DB 사본 경로> > remote_shown.tsv
열: 종목, 관련도, 영향도, 호재/악재, 같은 기사의 시장 관련도 최댓값(시장 연결이 없으면 0.0), T/S(이름·별칭이 제목에 있으면 T, 요약에만 있으면 S), 발행(한국 시간), 제목
"""
import json
import sqlite3
import sys

db = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
rows = db.execute(
    """
    SELECT s.name_ko, s.name, s.aliases, e.relevance, e.magnitude, e.impact,
           (SELECT MAX(e2.relevance) FROM article_matches m2
              JOIN stocks s2 ON s2.id = m2.stock_id
              JOIN evaluations e2 ON e2.match_id = m2.id
             WHERE m2.article_id = a.id AND s2.kind = 'market'),
           datetime(a.published_at / 1000, 'unixepoch', '+9 hours'), a.title
      FROM article_matches m
      JOIN stocks s ON s.id = m.stock_id
      JOIN evaluations e ON e.match_id = m.id
      JOIN articles a ON a.id = m.article_id
     WHERE m.match_kind = 'name' AND s.kind = 'stock'
       AND e.relevance >= 6 AND COALESCE(e.magnitude, 10) >= 5
     ORDER BY s.name_ko, e.relevance
    """
)
for name_ko, name, aliases, rel, mag, impact, market_rel, published, title in rows:
    names = [n for n in [name_ko, name, *json.loads(aliases or "[]")] if n]
    squashed = title.lower().replace(" ", "")
    where = "T" if any(n.lower().replace(" ", "") in squashed for n in names) else "S"
    print("\t".join([
        name_ko or name, f"{rel:.1f}", f"{mag:.1f}" if mag is not None else "", impact or "",
        f"{market_rel or 0:.1f}", where, published, title.replace("\t", " "),
    ]))
