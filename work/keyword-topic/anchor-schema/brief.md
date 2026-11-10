# anchor-schema: 마이그레이션 0011과 별칭·고유어 저장·검증
모델: Sonnet / effort: medium | 선행: 없음

## 목표
DB에 별칭·고유어·지운 별칭·토픽 범위·걸러진 연결 표시 칸을 더하고, 읽기·쓰기 함수와 별칭 검증 함수를 만든다. 아직 아무 곳에서도 쓰지 않는다(다음 작업이 씀).

## 완료 조건
- `migrate_tests`(또는 `schema_tests`) 새 시험 통과: 빈 DB와 0010까지 적용된 DB 모두 0011이 적용되고 새 칸의 기본값이 맞으며, 기존 종목(kind=stock, 지우지 않은 것)의 `keywords_status`가 `pending`이 됨
- `repository_tests` 새 시험 통과: 별칭·고유어 쓰기·읽기, 별칭 지우기(목록에서 빠지고 `aliases_removed`에 들어감)
- 별칭 검증 시험 통과(아래 규칙 하나마다 하나)
- `topics_tests`(또는 store 시험) 새 시험 통과: `Topic`이 `scope`를 읽고 기본값 `market`
- `cd rust && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` 통과

## 수정 가능 범위
- `rust/ripple/migrations/0011_keyword_anchors.sql`(새 파일)
- `rust/ripple/src/server/db/`
- `rust/ripple/src/server/stocks/repository.rs`
- `rust/ripple/src/server/stocks/repository_tests.rs`
- `rust/ripple/src/server/stocks/`(새 파일 `aliases.rs`·`aliases_tests.rs`, `stocks.rs`의 `mod` 줄)
- `rust/ripple/src/server/stocks.rs`
- `rust/ripple/src/server/topics/`
- `rust/ripple/src/test_support.rs`
- `rust/ripple/src/server/timeline/read_tests.rs`
- `rust/ripple/src/server/keywords/read_tests.rs`
- `rust/ripple/src/server/keywords/generate_tests.rs`

## 볼 위치
- `server/db/migrate.rs`의 `MIGRATION_SQL`(길이 11 고정 배열)
- `migrations/0007_topics_scores.sql`(칸 더하기 모양)
- `server/stocks/repository.rs`의 `Stock`, `find_stock`
- `server/topics/store.rs`의 `Topic`, `TOPIC_COLUMNS`, `row_to_topic`
- `listings` 표(`migrations/0001_stocks_listings_usage.sql`)
- `docs/DECISIONS.md` 1장 "17단계 `keyword-topic` 계획 답" ②⑦⑧, "17단계 계획 때 정한 세부" ⑤⑥

## 미리 정한 판단
- 0011 내용(기존 칸은 바꾸지 않음):
  - `stocks`에 `aliases`·`anchors`·`aliases_removed` (`text DEFAULT '[]' NOT NULL`, JSON 글자 목록)
  - `topics`에 `scope` (`text DEFAULT 'market' NOT NULL`)
  - `article_matches`에 `gated` (`integer DEFAULT 0 NOT NULL`, 1이면 고유어 조건에 걸러져 평가하지 않는 연결)
  - `UPDATE stocks SET keywords_status = 'pending', keywords_error = NULL WHERE kind = 'stock' AND deleted_at IS NULL`
  이유: 사용자 답 2A·7B·8A. 배포 직후 worker가 시작할 때 `stocks_awaiting_keywords`로 1회 다시 만든다. 버린 대안: worker 시작 코드에서 고유어 없는 종목을 고르기
- `Stock`에 `aliases`·`anchors`·`aliases_removed: Vec<String>`을 더한다. JSON이 깨졌으면 빈 목록으로 읽는다.
  이유: `topics.terms`와 같은 방식이다.
- 쓰기 함수: `set_stock_terms(conn, stock_id, aliases, anchors)`(두 목록을 바꿈), `remove_alias(conn, stock_id, alias) -> bool`(목록에서 빼고 `aliases_removed`에 더함, 없던 별칭이면 false). 이름은 바꿔도 된다.
- 별칭 검증 `validate_aliases(conn, stock, candidates) -> Vec<String>`. 순서대로 앞뒤 공백을 다듬고 아래를 버린 뒤 최대 5개:
  - 2자 미만
  - 종목 자기 이름(`name`·`name_ko`·`code`)과 같은 것(공백 빼고 소문자로 비교)
  - `listings`의 다른 종목(같은 시장·코드가 아닌 모든 줄) 이름과 같거나, 그 이름을 포함하거나, 그 이름에 포함되는 것(공백 빼고 소문자로 비교)
  - `aliases_removed`에 있는 것, 이미 고른 것과 같은 것
  이유: 실측에서 LLM이 'HDC현대산업개발'·'삼성' 같은 다른 회사 이름을 냈다(DECISIONS 2장 "유료 확인" 별칭 오류). 버린 대안: 사용자 확인 뒤 저장
- `Topic`에 `scope: String`(또는 market/company 열거형)을 더한다. 모르는 값은 market으로 읽는다.
- 0011은 drizzle가 없어진 뒤 처음 더하는 마이그레이션이다. drizzle 대조 시험은 "drizzle 시대(앞 `DRIZZLE_APPLIED_COUNT`=11개)"만 보게 범위를 좁히고, 0011은 따로 시험한다. `migrate_tests.rs`의 아래 4개만 이렇게 고친다(다른 기존 시험의 기대값은 그대로).
  - `migration_sql_files_are_identical_to_drizzle_folder`: 고정 복사본 개수를 `DRIZZLE_APPLIED_COUNT`와 비교하고, `MIGRATION_SQL[..DRIZZLE_APPLIED_COUNT as usize]`와만 내용 비교
  - `creates_db_file_and_parent_folder_with_settings_table`: `user_version`을 `MIGRATION_SQL.len()`(=12)과 비교(숫자 고정 대신)
  - `schema_applied_by_rust_equals_schema_applied_by_drizzle`: Rust 쪽은 `apply_migration_set(.., &MIGRATION_SQL[..11])`로 적용해 비교
  - `drizzle_db_gets_baseline_once_and_schema_is_unchanged`: `apply_migration_set(.., &MIGRATION_SQL[..11])`를 두 번 돌려 지금 뜻(스키마 그대로, user_version 11, `__drizzle_migrations` 11줄) 유지
  - 새 시험 하나 더: drizzle로 적용한 DB에 `apply_migrations`(전체) → user_version 12, 스키마가 새 파일 DB에 전체 적용한 것과 같음, `__drizzle_migrations` 11줄 그대로
  이유: drizzle는 0011을 적용한 적이 없으므로 고정 복사본·`applied.sql`은 0010까지의 기록으로 둔다. 버린 대안: fixtures·`applied.sql`에 0011 더하기(drizzle가 만든 적 없는 기록을 지어냄)

## 체크포인트 (각각 검사 통과 후 커밋)
1. 0011과 `MIGRATION_SQL` 길이 12 → 마이그레이션 새 시험 통과
2. `Stock`·`Topic` 새 칸과 쓰기 함수 → `repository_tests`·store 새 시험 통과
3. 별칭 검증 → 별칭 검증 시험 통과

## 제약
- 기존 시험의 기대값은 바꾸지 않는다(예외: 위 "미리 정한 판단"의 `migrate_tests.rs` 4개). `topics/`·`test_support.rs`·위 시험 파일은 구조체 칸이 늘어 깨지는 생성 코드만 고친다(`topics/store.rs`의 `Topic` 칸은 예외).
- drizzle 기준선(`DRIZZLE_APPLIED_COUNT` 등)과 `tests/fixtures/drizzle/`(`applied.sql`·`meta` 포함)는 건드리지 않는다. `MIGRATION_SQL` 위 설명 주석은 "0000~0010은 drizzle 복사본, 0011부터 Rust 전용"으로 고쳐도 된다.
- `docs/`는 고치지 않는다(인계에 적음).

## 알려진 함정
- `listings`에는 종목 자기 줄도 있다. 같은 시장·코드인 줄은 빼고 비교한다.
- `listings` 한국 줄은 수천 개다. 검증은 이름 목록을 한 번 읽어 메모리에서 비교한다.
- 시도 1에서 0011을 더하자 `migrate_tests.rs`의 drizzle 대조 시험 4개가 깨졌다(복사본 개수 11≠12, user_version 12≠11, 스키마 비교 2개). 고치는 방법은 "미리 정한 판단"의 마지막 항목대로. 시도 1이 남긴 미커밋 `migrations/0011_keyword_anchors.sql`·`migrate.rs`(길이 12)는 브리프와 맞으면 그대로 써도 된다.
- 진단(2026-11-10, 시도 1, sonnet/medium): 브리프 빈틈 — 첫 Rust 전용 마이그레이션이 drizzle 대조 시험을 깨는 것을 브리프가 미리 정하지 않음 → 브리프 빈틈

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고) + 마이그레이션 업그레이드 시험(`tests/fixtures/drizzle`)이 깨질 때
