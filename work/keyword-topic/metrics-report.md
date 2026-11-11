## 역할·모델별
| 역할 | 모델 | 세션 | 입력 토큰 | 출력 토큰 | 비용 | 캐시 없을 때 | 할인율 |
|---|---|---|---|---|---|---|---|
| diagnose | claude-opus-5-5 | 4 | 1184842 | 10396 | $1.2802 | $4.9472 | 74.1% |
| review | claude-opus-5-5 | 10 | 17359198 | 110939 | $10.3374 | $71.6558 | 85.6% |
| run | claude-sonnet-5-5 | 1 | 8991893 | 30631 | $2.0910 | $18.2901 | 88.6% |
| work | claude-sonnet-5-5 | 14 | 41114890 | 348574 | $11.1207 | $85.7157 | 87.0% |

관리 비용 / 구현 비용: 1.23
닫기 비용: -

## 최대 컨텍스트
| 역할 | 최소 | 중앙값 | 최대 |
|---|---|---|---|
| diagnose | 46302 | 53176 | 57917 |
| review | 84788 | 95895 | 143104 |
| run | 145113 | 145113 | 145113 |
| work | 49151 | 92022 | 217425 |

## 작업별
| 작업 | 시도 | 모델 순서 | 리뷰 결과 | 범위 밖 무기록 | 보고 |
|---|---|---|---|---|---|
| work/keyword-topic/anchor-match | 1 | claude-sonnet-5-5/medium | 통과 | 0 | 0 |
| work/keyword-topic/anchor-schema | 2 | claude-sonnet-5-5/medium → claude-sonnet-5-5/medium | 통과 | 0 | 1 |
| work/keyword-topic/eval-merge | 1 | claude-sonnet-5-5/medium | 통과 | 0 | 0 |
| work/keyword-topic/llm-request | 1 | claude-sonnet-5-5/medium | 통과 | 0 | 0 |
| work/keyword-topic/paid-compare | 2 | claude-sonnet-5-5/medium → claude-sonnet-5-5/medium | 통과 | 0 | 1 |
| work/keyword-topic/stage-check | 1 | claude-sonnet-5-5/medium | 통과 | 0 | 0 |
| work/keyword-topic/topic-input | 1 | claude-sonnet-5-5/medium | 통과 | 0 | 0 |
| work/keyword-topic/tree-full | 1 | claude-sonnet-5-5/medium | 통과 | 0 | 0 |
| work/keyword-topic/tree-screen | 3 | claude-sonnet-5-5/medium → claude-sonnet-5-5/medium → claude-sonnet-5-5/medium | 통과 | 1 | 2 |
| work/keyword-topic/tree-weekly | 1 | claude-sonnet-5-5/medium | 통과 | 0 | 0 |

반려율: 0/10, 단계 올림: 0회, 올린 뒤 통과: 0건

## 모델별 범위 밖 무기록
| 모델 | 리뷰 | 범위 밖 무기록 |
|---|---|---|
| claude-sonnet-5-5 | 10 | 1 |
