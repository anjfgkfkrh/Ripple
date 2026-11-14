# relink-7d 인계
커밋: 0001baf | 상태: 완료

## 검사 결과 (실행한 명령과 출력 숫자)
| 명령 | 출력 숫자 |
|---|---|
| cargo fmt --check | 통과 |
| cargo clippy --all-targets -- -D warnings | 경고 0 |
| cargo test | 741 passed, 0 failed (새 시험 4: backfill 3, worker/index 1) |
| ./build.sh --release | 성공 |
| pnpm format:check | 실패 1개: work/article-classify/measure/first-response.json (브리프에 적힌 기존 실패, 고치지 않음) |
| pnpm lint / typecheck | 통과 |
| pnpm test | 7 passed |
| pnpm test:e2e | 49 passed, 5 skipped, 0 failed (e2e 수정 없음) |

## 브리프에 없던 결정
- 시작 함수 시험이 없어 `worker/index.rs`에 `relink_markets_at_start(&Db)`(다시 걸기 함수를 부르고 로그 한 줄을 남기는 작은 함수)와 그 시험 모듈을 추가했다. 실패하면 로그만 남기고 시작은 계속한다.
- 다시 걸기는 `save_items` 대신 `article_matches`에 직접 INSERT(`ON CONFLICT DO NOTHING`, 유니크 인덱스 article_id+stock_id). 기사는 articles에서 직접 읽는다(`feed_headlines` 아님).
- 표시 설정 상수 이름: `MARKET_RELINKED_AT`(`classify.marketRelinkedAt`). 한 번만 하는 함수는 `relink_market_articles_once`.
- 새 연결의 created_at은 실행 시각(now).

## 발견한 것
- 작업 폴더 밖 untracked `work/metrics.csv`, `work/article-classify/metrics.csv`는 이미 있던 것이라 커밋하지 않았다.

## 상위로 올린 것
없음
## 다음 작업에 영향
- 다음 수집 회차가 새 시장 연결을 평가 대기로 평가한다(AI 호출은 그 회차에서).
