# schema: 마이그레이션 0012와 읽기 구조
모델: Sonnet / effort: medium | 선행: measure 통과

## 목표
이 단계의 새 칸을 마이그레이션 0012 하나로 더한다. 기사의 성격 확률 6칸과 물은 시각, 종목 평가의 `bystander` 확률이다. 쓰는 코드는 뒤 작업이 만든다.

## 완료 조건
- `migrate_tests` 새 시험 통과: 0011까지 있는 DB에 0012를 적용하면 아래 칸이 생기고 기존 줄은 모두 NULL, `user_version` 13
- `schema_tests`(또는 그 모듈 시험) 새 시험 통과: 성격 확률에서 태그·숨김을 고르는 함수(아래 판단)
- `cargo` 검사(서식·린트·단위) 전체 통과

## 수정 가능 범위
- `rust/ripple/migrations/0012_article_nature.sql`
- `rust/ripple/src/server/db/`
- `rust/ripple/src/test_support.rs`(시험 보조에 새 칸이 필요할 때만)

## 볼 위치
- `rust/ripple/migrations/0011_keyword_anchors.sql`(Rust 전용 마이그레이션 모양)
- `rust/ripple/src/server/db/migrate.rs`의 `MIGRATION_SQL`, `migrate_tests.rs`(0011 시험)
- `rust/ripple/src/server/db/schema.rs`의 `Impact`·`MatchKind`(문자열 enum 모양)

## 미리 정한 판단
- 칸(모두 NULL 허용, 기본값 없음):
  - `articles`: `nature_fact`, `nature_analysis`, `nature_forecast`, `nature_opinion`, `nature_media`, `nature_roundup`(REAL, 0~1), `nature_at`(INTEGER, 물었거나 복사한 시각, NULL이면 아직 없음)
  - `evaluations`: `bystander`(REAL, 0~1, NULL이면 이 질문 전의 평가이거나 종목이 아닌 항목)
  이유: 계획 답 ②(확률 그대로, 기준은 코드 상수). 버린 대안: 태그 글자 칸, 성격 별도 표
- 인덱스는 더하지 않는다.
  이유: 타임라인 조회는 기존 `evaluations` 인덱스에서 기사로 붙고, 새 칸은 그 뒤 거르기에만 쓴다.
- `schema.rs`(또는 `db/` 안 알맞은 곳)에 다음을 둔다:
  - 상수 `NATURE_THRESHOLD = 0.5`, `BYSTANDER_THRESHOLD = 0.5`
  - enum `NatureTag { Fact, Analysis, Forecast, Other }`(화면 글자 "사실"·"해석"·"전망"·"기타")
  - 성격 6확률을 받아 태그 목록(Fact·Analysis·Forecast는 각각 기준 이상이면, Other는 `opinion`이 기준 이상이거나 앞의 셋이 모두 기준 미만이면, 순서는 Fact·Analysis·Forecast·Other)과 숨김 여부(`media` 또는 `roundup`이 기준 이상)를 돌려주는 함수
  이유: 결정 ⑤⑭, 계획 답 ③. 탭·표시·저장이 같은 규칙을 쓰게 한 곳에 둔다.

## 체크포인트 (각각 검사 통과 후 커밋)
1. 0012와 목록 등록 → `migrate_tests` 새 시험 통과
2. 기준 상수·태그 enum·고르기 함수 → 새 단위 시험 통과

## 제약
- 0000~0010(drizzle 복사본)은 고치지 않는다.
- 다른 모듈의 동작은 바꾸지 않는다.

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고)
