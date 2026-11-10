# anchor-schema 보고
> 사실만 쓴다.

## 멈춘 이유 (해당하는 보고 조건)
- 기대값 변경 필요 + 마이그레이션 업그레이드 시험(`tests/fixtures/drizzle`)이 깨짐. 0011을 더하고 `MIGRATION_SQL` 길이를 12로 하면 기존 `migrate_tests.rs` 시험 4개가 실패한다. 브리프 제약은 "기존 시험의 기대값은 바꾸지 않는다"이고, `migrate_tests.rs`는 수정 가능 범위(`server/db/`)에 있으나 기대값 자체를 바꿔야 통과한다.

## 재현 방법
1. `rust/ripple/migrations/0011_keyword_anchors.sql` 작성(브리프 "미리 정한 판단"의 칸 5개 + UPDATE), `migrate.rs`의 `MIGRATION_SQL`을 `[&str; 12]`로 하고 0011 include 추가.
2. `cd rust/ripple && cargo test migrate`

## 오류 출력
```
migration_sql_files_are_identical_to_drizzle_folder FAILED  (migrate_tests.rs:161  assert_eq names.len() == MIGRATION_SQL.len(): left 11, right 12)
  -> tests/fixtures/drizzle 에는 .sql이 11개뿐(0011 없음)
creates_db_file_and_parent_folder_with_settings_table FAILED (migrate_tests.rs:62 user_version left 12, right 11)
drizzle_db_gets_baseline_once_and_schema_is_unchanged FAILED (migrate_tests.rs:203 schema_dump(after) != before; 0011이 칸을 더함. 같은 시험이 user_version == 11도 기대(:205))
schema_applied_by_rust_equals_schema_applied_by_drizzle FAILED (migrate_tests.rs:188 rust 적용 스키마 != applied.sql 스키마; 0011 칸 때문)
test result: FAILED. 5 passed; 4 failed
```

## 시도한 것
- 위 1~2만 했다. 고치지 않았다(기대값 변경과 fixtures 관련이라 규칙상 직접 수정 불가).
- 필요해 보이는 변경(판단은 상위): user_version 기대 11→12, drizzle 고정 복사본 비교를 앞 11개로 제한, 스키마 동일 비교·업그레이드 시험을 0010 기준선 DB에서 0011까지 적용된 결과로 바꾸기, `applied.sql`/fixtures에 0011을 더할지 여부.

## 마지막 커밋과 현재 상태
- 마지막 커밋: 18b1fb2(작업 시작 시점, 새 커밋 없음). 끝낸 체크포인트: 없음.
- 작업 트리에 미커밋 변경 2개 남김: `rust/ripple/migrations/0011_keyword_anchors.sql`(새 파일), `rust/ripple/src/server/db/migrate.rs`(길이 12 + include).
