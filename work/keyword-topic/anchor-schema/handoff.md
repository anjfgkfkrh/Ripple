# anchor-schema 인계
커밋: ef21127 | 상태: 완료

## 검사 결과 (실행한 명령과 출력 숫자)
| 명령 | 출력 숫자 |
|---|---|
| cargo fmt --check | 통과(출력 없음) |
| cargo clippy --all-targets -- -D warnings | 경고 0 |
| cargo test | 652 passed; 0 failed (시도 시작 전 639) |
| cargo test migrate | 11 passed (새 시험 2개 포함) |

## 브리프에 없던 결정
- `Topic.scope`는 `String`(`market`/`company`, 그 밖의 값은 `market`으로 읽음). 열거형 아님.
- `remove_alias`는 목록에 없던 별칭이면 false(`aliases_removed`에도 안 더함). 있으면 `aliases_removed`에 중복 없이 추가.
- `set_stock_terms`는 없는 종목이면 false를 돌려줌(bool).
- `validate_aliases`는 `Result<Vec<String>, rusqlite::Error>`를 돌려줌. 위치: `server/stocks/aliases.rs`, 상수 `MAX_ALIASES=5`. 결과의 별칭은 다듬은(trim) 원문 그대로(정규화 전).
- 이 모듈의 시험 파일 `aliases_tests.rs`는 `repository.rs`와 같은 `#[path]` 방식으로 `aliases.rs`에서 선언(`stocks.rs`에는 `pub mod aliases;`만 추가).
- `migrate_tests.rs` 4개를 브리프대로 고침 + 새 시험 2개(`drizzle_db_gets_rust_only_migrations_after_baseline`, `migration_0011_adds_anchor_columns_and_resets_stock_keywords`). `rerun_skips_already_applied` 등 나머지는 그대로. 시험이 쓰는 `Topic` 생성 코드 2곳(topics_tests.rs)에 `scope` 칸만 더함.
- 범위 밖 파일 변경 없음(`work/metrics.csv`·`work/keyword-topic/metrics.csv`는 harness가 쓴 것, 커밋하지 않음).

## 발견한 것 (함정, 실패 교훈, 기존 코드의 버그)
- 마이그레이션 시험이 `stocks`에 직접 넣을 때 `exchange`·`user_input`이 NOT NULL이고 id는 integer.
- 검증은 정규화한 listings 이름을 한 번 읽어 메모리 비교. 영문 이름에 포함 규칙이 걸리므로(예: 짧은 영문 별칭이 어떤 미국 종목 이름에 포함) 실제 데이터에서 별칭이 많이 버려질 수 있다.

## 상위로 올린 것
없음

## 다음 작업에 영향
- 아직 아무 곳에서도 새 칸·함수를 쓰지 않음. `Stock`에 `aliases`·`anchors`·`aliases_removed`, `Topic`에 `scope`, `article_matches.gated` 칸이 생김.
- 문서 갱신 필요(상위): SPEC·DEVELOPMENT 코드 지도에 `stocks/aliases.rs`, 마이그레이션 0011(Rust 전용 첫 마이그레이션).
