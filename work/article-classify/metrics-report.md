## 역할·모델별
| 역할 | 모델 | 세션 | 입력 토큰 | 출력 토큰 | 비용 | 캐시 없을 때 | 할인율 |
|---|---|---|---|---|---|---|---|
| plan | claude-opus-5-5 | 1 | 10588139 | 83748 | $5.9831 | $43.6246 | 86.3% |
| review | claude-opus-5-5 | 9 | 10761768 | 74261 | $7.1170 | $44.5321 | 84.0% |
| run | claude-sonnet-5-5 | 1 | 5381693 | 16187 | $1.3444 | $10.9253 | 87.7% |
| work | claude-sonnet-5-5 | 9 | 25133434 | 181740 | $6.3220 | $52.0843 | 87.9% |

관리 비용 / 구현 비용: 2.28
닫기 비용: -

## 최대 컨텍스트
| 역할 | 최소 | 중앙값 | 최대 |
|---|---|---|---|
| plan | 424290 | 424290 | 424290 |
| review | 73088 | 76516 | 119773 |
| run | 110688 | 110688 | 110688 |
| work | 55220 | 87362 | 166016 |

## 작업별
| 작업 | 시도 | 모델 순서 | 리뷰 결과 | 범위 밖 무기록 | 보고 |
|---|---|---|---|---|---|
| work/article-classify/article-nature | 1 | claude-sonnet-5-5/medium | 통과 | 0 | 0 |
| work/article-classify/display-rule | 1 | claude-sonnet-5-5/medium | 통과 | 0 | 0 |
| work/article-classify/market-alias | 1 | claude-sonnet-5-5/medium | 통과 | 0 | 0 |
| work/article-classify/measure | 1 | claude-sonnet-5-5/medium | 통과 | 0 | 0 |
| work/article-classify/nature-tabs | 1 | claude-sonnet-5-5/medium | 통과 | 0 | 0 |
| work/article-classify/relink-7d | 1 | claude-sonnet-5-5/medium | 통과 | 0 | 0 |
| work/article-classify/schema | 1 | claude-sonnet-5-5/medium | 통과 | 0 | 0 |
| work/article-classify/stage-check | 1 | claude-sonnet-5-5/medium | 통과 | 0 | 0 |
| work/article-classify/stock-subject | 1 | claude-sonnet-5-5/medium | 통과 | 0 | 0 |

반려율: 0/9, 단계 올림: 0회, 올린 뒤 통과: 0건

## 모델별 범위 밖 무기록
| 모델 | 리뷰 | 범위 밖 무기록 |
|---|---|---|
| claude-sonnet-5-5 | 9 | 0 |
