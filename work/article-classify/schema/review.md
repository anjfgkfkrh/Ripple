# article-classify/schema 리뷰
결과: 통과
범위 밖 무기록: 0

## 범위 확인
- 검사 명령 재실행: 인계 숫자와 일치. `cargo test --lib migrate` 12 passed, `cargo test --lib` 729 passed, `cargo fmt --check` 통과, `cargo clippy --all-targets` 경고 없음.
- 범위 밖 판단: 없음(바뀐 파일은 `migrations/0012_article_nature.sql`, `server/db/` 안 4개, 작업 폴더·PLAN.md뿐. 미추적 `work/metrics.csv`·`work/article-classify/metrics.csv`는 작업 전부터 있음)

## Opus 확인
- 브리프에 없던 결정이 인계에 빠짐없이 있는가: 예. `NatureProbs`·`classify_nature` 이름, `user_version` 기대값 12 → 13, 기존 줄 NULL 확인 방식.
- 테스트 기대값을 바꾸거나 빼서 통과시키지 않았는가: 예. 바뀐 기대값은 두 시험의 `user_version` 12 → 13뿐이고, 마이그레이션이 하나 늘어난 직접 결과라 정당하다.
- 제약과 프로젝트 규칙을 지켰는가: 예. 0000~0011 변경 없음, 인덱스 없음, 칸 타입·NULL 허용이 브리프와 같음, 다른 모듈 동작 변경 없음. 고르기 함수 규칙(경계 0.5 포함, 순서, Other 조건, 숨김)이 브리프와 같고 시험이 경계값(0.5, 0.49)을 덮는다.
- 인계가 실제 변경과 맞는가: 예.
- 참고: 완료 조건 "기존 줄은 모두 NULL"은 줄을 넣어 확인하지 않고 칸 존재만 본다. SQLite `ALTER TABLE ADD` 기본값 없는 칸은 기존 줄이 NULL이라 동작상 문제는 없어 통과로 본다.

## 반영한 것 (통과일 때)
- 영구 문서: DECISIONS 1장 새 줄 "18단계 schema 구현 세부"(①~③, 근거: article-classify/schema). STATE 다시 씀(schema 통과, 다음은 market-alias, stock-subject는 사람 확인 대기 그대로). SPEC 변경 없음.
- 다음 브리프 확인: `market-alias`는 이번 변경과 관계없어 그대로. `article-nature`(대기)에 `classify_nature`·`NatureProbs` 이름 겹침 함정 한 줄, `nature-tabs`(대기)에 함수 이름과 성격 없는 기사를 넣지 말 것 함정 한 줄.
