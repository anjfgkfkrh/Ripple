## 역할·모델별
| 역할 | 모델 | 세션 | 입력 토큰 | 출력 토큰 | 비용 | 캐시 없을 때 | 할인율 |
|---|---|---|---|---|---|---|---|
| review | claude-opus-5-5 | 4 | 9200592 | 55699 | $4.9962 | $37.9163 | 86.8% |
| run | claude-sonnet-5-5 | 1 | 7336516 | 30745 | $1.4258 | $14.9805 | 90.5% |
| work | claude-sonnet-5-5 | 4 | 9516575 | 75671 | $2.5129 | $19.7899 | 87.3% |

관리 비용 / 구현 비용: 2.56
닫기 비용: -

## 최대 컨텍스트
| 역할 | 최소 | 중앙값 | 최대 |
|---|---|---|---|
| review | 89076 | 106533 | 217070 |
| run | 133548 | 133548 | 133548 |
| work | 64906 | 105343 | 114246 |

## 작업별
| 작업 | 시도 | 모델 순서 | 리뷰 결과 | 범위 밖 무기록 | 보고 |
|---|---|---|---|---|---|
| work/separate-role/access-token | 1 | claude-sonnet-5-5/medium | 통과 | 0 | 0 |
| work/separate-role/guest-limits | 1 | claude-sonnet-5-5/medium | 통과 | 1 | 0 |
| work/separate-role/role-guard | 1 | claude-sonnet-5-5/medium | 통과 | 0 | 0 |
| work/separate-role/unlock-ui | 1 | claude-sonnet-5-5/medium | 통과 | 0 | 0 |

반려율: 0/4, 단계 올림: 0회, 올린 뒤 통과: 0건

## 모델별 범위 밖 무기록
| 모델 | 리뷰 | 범위 밖 무기록 |
|---|---|---|
| claude-sonnet-5-5 | 4 | 1 |
