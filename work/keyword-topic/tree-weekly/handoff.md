# tree-weekly 인계
커밋: 17ae2e1 | 상태: 완료

## 검사 결과 (실행한 명령과 출력 숫자)
| 명령 | 출력 숫자 |
|---|---|
| `cargo test` (`/check` R3) | 698 통과, 0 실패 (이전 672 → 새 시험 26개) |
| `cargo fmt --check` (R1) | 오류 0 |
| `cargo clippy --all-targets -- -D warnings` (R2) | 경고 0 |
| `./build.sh --release` (R4) | 오류 0 |
| `pnpm format:check` / `pnpm lint` / `pnpm typecheck` | 오류 0 / 0 / 0 |
| `pnpm test` | 7 통과, 0 실패 |
| `pnpm test:e2e` | 49 통과, 0 실패, 5 건너뜀 |

새 시험: `performance_tests` 4, `generate_tests` 증분 11, `tree_tests` 3, `schemas_tests` 1, `queue_tests` 4, `plan_tests` 3(1개는 기존 `weekly_means_monday_six`를 대체).

## 브리프에 없던 결정
- 새 파일(수정 범위 `server/keywords/` 안): `performance.rs`(키워드 실적), `update.rs`(점검 반영 `apply_update`, `UpdateSummary`, `load_structure`, `tree_for_prompt`), `*_tests.rs`. `keywords.rs`에 `pub mod performance;`·`pub mod update;` 추가.
- 실적 함수 `keyword_performance(conn, stock_id, since)`: 기준은 연결(`article_matches.created_at`) 시각이 `since` 이상. 표 키는 `normalize_key`(공백 제거·소문자)한 글자. 평가가 없는 연결(gated 포함)은 걸린 수에만 센다. 마지막 통과 시각은 통과한 연결의 `created_at` 최댓값.
- 지키는 판단은 28일 실적(`since = now-28일`)으로, 상한 밀어내기 순서는 전체 기간 실적(`since = 0`)의 마지막 통과로 한다(브리프 "마지막 통과(없으면 만든 시각)"를 글자 그대로). 함수를 두 번 부른다.
- 뺄 것에서 같은 글자가 여러 줄이면 모두 같은 규칙으로 본다. 3단계 이상은 실적과 상관없이 뺀다.
- 더할 것 "최대 3개": 검증을 통과한 것 가운데 앞에서 3개(검증에 걸린 것은 3개에 세지 않음). 검증은 `tree.rs`의 `KeywordRules`(전체 생성 규칙을 뽑아냄)와 `accept_added_keywords`. 단계는 1·2만, 이미 남은 키워드와 겹치면 버림.
- 고유어: 더하기도 "이미 있는 고유어와 겹치는 것은 3개에 세지 않음". "0개가 되면 빼지 않음"은 빼기+더하기 결과가 비면 빼기를 취소하는 것으로 해석.
- 별칭: 있던 별칭은 그대로, 더할 것만 `validate_aliases`(최대 5)를 거쳐 중복 빼고 붙인다. 전체 개수 상한은 두지 않았다.
- `GenerateOk`에 `update: Option<Box<UpdateSummary>>` 칸 추가(clippy `result_large_err` 때문에 Box). 전체 생성이면 None. 기존 시험 리터럴 3곳(`generate_tests` 1, `queue_tests` 2)에 `update: None`만 더함.
- 점검은 사용량 목적 `keyword_generation` 그대로(예산 화면 이름 `키워드 만들기` 유지), 스키마 이름 `keyword_update`.
- 대기열: `KeywordQueue::enqueue_periodic` 추가, `enqueue`는 전체 생성. `KeywordEnqueuer` 트레이트에 `enqueue_periodic`을 기본 구현(전체 생성과 같음)으로 더해 worker 쪽 코드·가짜 대기열은 안 고침. 스케줄러만 `enqueue_periodic`을 부름.
- 정기 점검 결과 줄: `키워드 점검: 종목 id N, 지금 K개(뺌 …, 더함 …, 상한으로 밀어냄 …, 실적으로 지킴 …, 고유어 -a+b, 별칭 +c[, 규칙에 안 맞아 버림 d]), 초`. 스케줄러 로그는 `키워드 정기 점검(매일 06:00 확인, 주기 …): N개 종목`.
- 계획(`keyword_plan`): 확인(`due`)은 항상 매일 06:00 기준, 대상은 설정 주기 시작(하루: 오늘 06:00, 주: 월요일 06:00) 전에 만든 항목. `next_at`은 항상 다음 날 06:00. 설정 기본값·화면은 건드리지 않음.
- 프롬프트 고정 파일 `prompts/keyword_update.json`은 `llm_make.py`의 `update_messages`를 실제로 돌려 만들었다(사례 3: 한국 종목·미국 종목·시장. 주제 항목은 파이썬에 없어 사례 없음, Rust `target_words`의 주제 문구를 씀). 스키마 `keyword_update.json`도 `UPDATE_SCHEMA`를 그대로 덤프. 프롬프트의 "주 1회 점검" 문구는 그대로(매일 확인해도).
- 범위 밖: 없음(`git status`에 범위 밖 변경 없음, 이미 있던 `work/metrics.csv`는 건드리지 않음).
- 기대값 변경: `plan_tests`의 `weekly_means_monday_six`를 새 규칙 시험(`weekly_still_checks_every_day_at_six_but_targets_stocks_made_before_monday`)으로 바꿈. `scheduler_tests`는 기대값 그대로, `FakeKeywordQueue`에 `periodic` 기록과 한 줄 단언만 더함.

## 발견한 것 (함정, 실패 교훈, 기존 코드의 버그)
- 점검 프롬프트도 한글 문법 흠("요인로")이 파이썬 문구 그대로 들어감.
- 통계 낱말은 정기 점검에서도 종목마다 계산한다(`feed_headlines` 전체를 읽음). 브리프 함정대로 항목 수만큼 읽는다.
- 옛 트리의 3단계 이상 줄은 첫 점검에서 모두 사라진다(시험 있음).

## 상위로 올린 것
없음

## 다음 작업에 영향
- 설정 기본값(주 1회)·설정 화면 문구는 tree-screen이 고친다. 지금도 하루 1회 설정이면 매일 오늘 06:00 전 항목을 점검한다.
- `docs/`는 고치지 않았다(SPEC·DECISIONS 반영은 review): 위 "브리프에 없던 결정"이 반영 대상.
- 시험 Docker 확인 때 볼 것: 점검 한 번의 시간·비용, `validate_aliases`가 버린 별칭, 첫 점검에서 옛 트리가 상한으로 줄어드는 모습.
