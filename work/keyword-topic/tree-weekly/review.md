# tree-weekly 리뷰
결과: 통과
범위 밖 무기록: 0

## 범위 확인
- 검사 명령 재실행: 인계 숫자와 일치. fmt 0, clippy 경고 0, cargo test 698 통과·0 실패, build.sh --release 0, pnpm format/lint/typecheck 0, pnpm test 7 통과, E2E 49 통과·5 건너뜀.
- 범위 밖 판단: 없음. 바뀐 파일은 모두 수정 가능 범위 안이다(`server/keywords/`·`prompts/`·`schemas/`·`schemas.rs`·`schemas_tests.rs`·`schedule/`·`e2e/mock-services.mjs`·`test_support.rs`). `work/metrics.csv`·`work/keyword-topic/metrics.csv`의 커밋 안 된 변경은 run 기록이라 건드리지 않았다.

## Opus 확인
- 브리프에 없던 결정이 인계에 빠짐없이 있는가: 예.
  - 인계에 적힌 것: 새 파일 `performance.rs`·`update.rs`, 실적 키의 정규화, 28일 실적과 전체 기간 실적을 따로 쓰는 것, 3단계 이상 무조건 빼기, "최대 3개"를 세는 방식, 고유어 0개일 때의 해석, 별칭 전체 상한 없음, `GenerateOk.update`(Box), 사용량 목적, `enqueue_periodic`의 기본 구현, 결과 줄과 로그 문장, 계획의 `next_at`.
  - diff와 대조했고 빠진 것이 없다.
- 테스트 기대값을 바꾸거나 빼서 통과시키지 않았는가: 예.
  - 지운 시험은 `plan_tests`의 `weekly_means_monday_six` 하나다. 브리프 제약에서 허용한 "주 1회 월요일 확인 시각" 시험이고, 새 규칙 시험으로 바꿨다.
  - `scheduler_tests`는 기대값이 그대로다. `periodic` 기록과 단언만 더했다.
  - 기존 시험 리터럴 3곳에는 `update: None`만 더했다.
- 제약과 프로젝트 규칙을 지켰는가: 예.
  - 반영 순서는 브리프 1~6과 같다: 뺄 것·지킴, 1단계를 빼면 아래도 빠짐, 더할 것 3개와 검증, 상한 밀어내기(토픽 가지는 `topic_id IS NULL`로 제외), 고유어, 별칭, `ready`·`keywords_generated_at`. 모두 한 트랜잭션이다.
  - 바뀌지 않은 줄은 DELETE·INSERT하지 않는다.
  - 실패하면 `set_keyword_status`가 `keywords_generated_at`을 건드리지 않는다. 그래서 다음 날 06:00 확인 때 다시 고른다.
  - 계획은 `due`를 매일 06:00 경계로, 대상을 주기 경계로 고른다.
  - 대기열은 전체 생성이 정기 점검을 덮는다.
  - 실제 API는 부르지 않았고 `docs/`는 고치지 않았다.
- 인계가 실제 변경과 맞는가: 예.

## 남기는 말
- 작은 점이 두 가지 있다. 시험 Docker에서 이상이 보이면 고친다.
  - 답의 `remove`에 같은 글자가 두 번 오면 "실적으로 지킴" 수가 두 번 센다. 로그 숫자에만 영향이 있다.
  - 이번에 뺀 키워드를 같은 답의 `add`가 다시 넣으면 그대로 받는다. 이 경우 새 id·만든 시각이 된다.
- 상한 밀어내기는 28일 안에 통과가 있어 지킨 키워드도 밀어낼 수 있다. 브리프 순서(지킴 → 상한)대로다.

## 반영한 것 (통과일 때)
- 영구 문서:
  - DECISIONS 1장: "17단계 tree-weekly 구현 세부" 줄을 더했다(근거 포함).
  - 대체됨을 붙인 줄: "키워드 자동 재생성 시각", "키워드 자동 재생성 대상", "11단계 06:00 재생성".
  - SPEC FR-3: "시각" 하위 항목에 17단계 확인 규칙을 더했다. 정기 점검 항목에 전체 생성으로 바뀌는 조건과 옛 3단계 처리를 더했다.
  - STATE: 다시 썼다.
- 다음 브리프 확인:
  - tree-screen(다음, 대기): 알려진 함정에 한 줄을 더했다. 실적 함수 위치·서명, `normalize_key` 키, 설정 주기 칸 설명을 다룬다.
  - stage-check(대기): 알려진 함정에 정기 점검을 일으켜 재는 방법을 더했다.
