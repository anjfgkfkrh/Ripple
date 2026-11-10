# anchor-match 리뷰
결과: 통과
범위 밖 무기록: 0

## 범위 확인
- 검사 명령 재실행: 인계 숫자와 일치(`cargo fmt --check` 통과, `cargo clippy --all-targets -- -D warnings` 경고 0, `cargo test` 660 passed 0 failed)
- 범위 밖 판단:
  - 없음(바뀐 코드는 모두 브리프 "수정 가능 범위" 안, 그 밖은 PLAN·작업 폴더·`metrics.csv` 기록 파일)

## Opus 확인
- 브리프에 없던 결정이 인계에 빠짐없이 있는가: 예. `squash`(소문자·모든 공백 제거), 빈 고유어 버림, `json_list`, 시험 데이터 고유어 보정이 인계에 있다.
- 테스트 기대값을 바꾸거나 빼서 통과시키지 않았는가: 예. 기존 `match_tests`의 바뀐 줄은 `find(text)` → `find(text, "")`와 시험 종목의 `aliases`·`anchors` 칸뿐이고 기대값은 그대로다. `collect/run_tests`는 종목에 고유어를 SQL로 넣어 맞췄다(브리프 제약대로).
- 제약과 프로젝트 규칙을 지켰는가: 예. `docs/` 미수정, 걸러짐은 종목 종류 키워드 적중에만, 이름·별칭·직접·시장·주제는 걸러지지 않음.
- 인계가 실제 변경과 맞는가: 예. 다만 하나 빠뜨린 곳이 있어 리뷰가 고쳤다(아래).

## 리뷰가 직접 고친 것 (별도 커밋 b566693)
- `evaluate/` 안에 `count_pending_evaluations`가 둘 있다. worker는 `evaluate/run.rs` 것만 고쳤고, 타임라인 화면(`web/timeline.rs:601`)이 쓰는 `evaluate/read.rs` 것은 `gated` 조건이 없어 걸러진 연결이 "평가 대기" 수에 7일 동안 계속 잡혔다. `article_matches.gated = 0`을 더하고 `read_tests.rs` 기존 시험에 걸러진 연결 한 줄을 넣어 확인했다(고치기 전 실패, 고친 뒤 통과, 전체 660 passed).

## 반영한 것 (통과일 때)
- 영구 문서: DECISIONS 1장에 "17단계 anchor-match 구현 세부" 줄(근거: keyword-topic/anchor-match). STATE.md 다시 씀(anchor-match 통과, 지금은 고유어가 비어 종목 키워드 연결이 모두 걸러짐, 다음은 tree-full·eval-merge·topic-input). SPEC은 FR(116줄)·데이터 표(311줄)에 이미 있어 고치지 않음.
- 다음 브리프 확인: `tree-full`은 E2E 고유어 보정을 이미 다룬다. "알려진 함정"에 고유어 비교 규칙(`squash`)·저장된 `gated`는 다시 계산하지 않음·`collect.spec.ts` "하닉"을 더함. `eval-merge` "알려진 함정"에 평가 대기 질의 세 곳의 `gated = 0` 조건을 잃지 말라고 더함.
- 남은 것(결정 아님): 상태·타임라인에서 걸러진 연결을 따로 보일지는 다루지 않았다. 타임라인은 평가 행 기준이라 걸러진 연결은 보이지 않는다.
