"""공개 종목 화면(손님 보기)에서 지금 트리를 뽑는다. 사용: curl -s https://ripple.anjfgkfkrh.com/stocks > stocks.html; python3 -I page_trees.py stocks.html > trees.tsv
열: 경로, 단계, 4주 기사, 4주 통과, 토픽 머리(T), 이유"""
import html
import re
import sys

t = open(sys.argv[1]).read()
for m in re.finditer(r'<p class="text-sm" title="([^"]+)">(.*?)</p>\s*<p class="text-xs text-muted-foreground">(.*?)</p>', t, re.S):
    path = html.unescape(m.group(1))
    body = m.group(2)
    st = re.search(r"기사 (\d+) · 통과 (\d+)", body)
    topic = "topic-badge" in body
    print("\t".join([path, str(path.count("→")), st.group(1) if st else "", st.group(2) if st else "", "T" if topic else "", html.unescape(m.group(3))]))
