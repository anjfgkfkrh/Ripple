# anchor-schema 리뷰
결과: 통과
범위 밖 무기록: 0

## 범위 확인
- 검사 명령 재실행: 인계 숫자와 일치(fmt 통과, clippy 경고 0, cargo test 652 passed 0 failed)
- 범위 밖 판단:
  - 없음. `topics/topics_tests.rs`는 브리프 범위 `server/topics/` 안. `work/metrics.csv`·`work/keyword-topic/metrics.csv`는 harness 기록 파일

## Opus 확인
- 브리프에 없던 결정이 인계에 빠짐없이 있는가: 예 — `scope`를 String으로, `remove_alias`·`set_stock_terms`가 bool, `validate_aliases`가 Result이고 다듬은 원문을 돌려줌, `MAX_ALIASES`, 시험 파일 `#[path]` 방식. 모두 브리프가 허용한 작은 선택
- 테스트 기대값을 바꾸거나 빼서 통과시키지 않았는가: 예 — `migrate_tests.rs`는 브리프가 정한 4개만 정한 방식대로 고침(앞 11개 비교, user_version은 `MIGRATION_SQL.len()`), 그 시험의 나머지 단언(user_version 11, `__drizzle_migrations` 11줄)은 그대로. topics_tests는 생성 코드에 `scope`만 더함
- 제약과 프로젝트 규칙을 지켰는가: 예 — drizzle 고정본·`DRIZZLE_APPLIED_COUNT` 안 건드림, 0011은 기존 칸 안 바꿈, `docs/` 안 고침, listings는 한 번 읽어 메모리 비교, 자기 줄(같은 시장·코드) 뺌
- 인계가 실제 변경과 맞는가: 예 — diff와 일치. 새 시험: migrate 2개(drizzle DB에 전체 적용·0011 칸과 pending 재설정, 지운 종목·index 제외 확인), repository 3개, aliases 9개(규칙마다), topics 1개

## 반영한 것 (통과일 때)
- 영구 문서: DECISIONS 1장에 "17단계 anchor-schema 구현 세부" 줄 추가(근거 붙임), DEVELOPMENT 8장 코드 지도에 0011 Rust 전용·drizzle 대조 범위, `stocks/repository.rs`·`stocks/aliases.rs` 추가, STATE 다음 할 일 갱신
- 다음 브리프 확인: `anchor-match`는 이번 인계와 맞음(`Stock.aliases`·`anchors`, `article_matches.gated`), 고칠 것 없음. `tree-full`(대기) 알려진 함정에 `validate_aliases`의 Result·포함 비교로 별칭이 많이 버려질 수 있음·트랜잭션 안 `set_stock_terms` 한 줄 추가
- 남길 말: 포함 규칙은 `listings`에 아주 짧은 이름(1~2자 영문 등)이 있으면 그 글자를 담은 별칭을 모두 버린다. 실제 데이터에서 얼마나 버려지는지는 `stage-check`에서 본다
