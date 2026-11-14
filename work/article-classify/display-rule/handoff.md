# 인계: display-rule
커밋: c162896 | 상태: 완료

## 한 일
- `evaluate/run.rs`에 `NOT_HIDDEN_SQL`(숨김 조건 SQL 조각, `?` 하나에 `BYSTANDER_THRESHOLD`)을 두었다.
- `timeline/read.rs`의 `display_rule`이 조각을 AND로 붙이고 `?` 값 3개를 돌려준다. 이 조건을 쓰는 세 조회 가운데 `stocks` 조인이 없던 둘(토픽 카드의 항목 평가 조회 두 곳)에 `INNER JOIN stocks`를 더했다.
- `topics/store.rs` `topic_article_counts`의 `shown` 식에 조각을 붙이고 바인딩에 기준을 더했다.
- `keywords/performance.rs`·`stat_words.rs`는 `stocks`를 조인하고 `CASE WHEN <조각> THEN 1 ELSE 0 END` 칸을 읽어 `meets_display_rule && 숨김 아님`으로 센다(`meets_display_rule` 서명 그대로).
- 평가 실행 요약 `relevant`는 건드리지 않았다.
- 새 시험: 타임라인 3개(bystander 0.7/0.3, NULL 시장 비교 9>7·7≤7·시장 없음, 종목 필터, 시장·주제 항목 제외), 토픽 수 1개, 키워드 통과 1개, 통계 낱말 1개.

## 검사 결과
| 명령 | 출력 |
|---|---|
| cargo test | 752 통과, 0 실패 |
| /check (fmt, clippy, cargo test, build, pnpm 형식·린트·타입·단위, E2E) | 전부 통과. E2E 49 통과, 0 실패, 5 건너뜀(모바일) |

## 깨진 기존 시험
- 없음(새 규칙으로 깨진 기존 시험이 없어 기존 시험 준비는 고치지 않음).

## 브리프에 없던 결정
- `keyword_performance`·`stat_words` SQL은 `?1`·`?2` 번호 자리표와 조각의 `?`가 섞이면 번호가 겹치므로, 조각의 `?`를 `?3`으로 바꿔 넣는다(`NOT_HIDDEN_SQL.replace('?', "?3")`).
- `keyword_performance` 쿼리에 `INNER JOIN stocks ON stocks.id = article_matches.stock_id`를 더했다(연결의 종목 칸은 NOT NULL이라 행이 줄지 않는다고 봄, 시험 통과).
- 토픽 수 시험의 시장 토픽 항목 평가 쪽은 `bystander` 0.9여도 `shown`에 센다는 것을 확인했다.
- 타임라인 조회 속도는 재지 않았다.
