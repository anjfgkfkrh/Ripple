import csv, re, sys, collections
rows = [r for r in csv.reader(open('remote_shown.tsv', encoding='utf-8'), delimiter='\t')]
MKT = re.compile(r'코스피|코스닥|증시|지수|나스닥|S&P|닛케이|사이드카|서킷|[56]천피|5000선|마감|시황|개장|출발|장 초반|장초반|마켓뷰|마켓워치|투자전략|외국인|외인|개미|투매|프리장|프리마켓')
DIG = re.compile(r'주요뉴스|이슈 종합|주요기사|이슈픽|브리핑\]|\(13일\)')
out = collections.defaultdict(list)
for name, rel, mag, imp, mrel, where, when, title in rows:
    r = float(rel)
    if DIG.search(title): out[('②', name)].append(r); print('②', name, rel, where, title); continue
    if MKT.search(title) and r < 9.5:
        b = '①' if where == 'S' else '④'
        out[(b, name)].append(r); print(b, name, rel, mrel, where, when[:10], title)
for k in sorted(out): print(k, len(out[k]))
