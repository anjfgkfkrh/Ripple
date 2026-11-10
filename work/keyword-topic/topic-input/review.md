# topic-input 리뷰
결과: 통과
범위 밖 무기록: 0

## 범위 확인
- 검사 명령 재실행: 인계 숫자와 일치(fmt 차이 0, clippy 경고 0, cargo test 727 통과·0 실패, release 빌드 오류 0, pnpm format/lint/typecheck 0/0/0, pnpm test 7 통과, E2E 49 통과·5 건너뜀)
- 범위 밖 판단: 없음(바뀐 파일 모두 브리프 수정 가능 범위 안. `work/metrics.csv`는 미추적 파일로 이 작업 변경 아님)

## Opus 확인
- 브리프에 없던 결정이 인계에 빠짐없이 있는가: 예. `parse_scope` 빠진 값도 market, 관련 종목 없는 company 토픽은 이름·id 기록 전에 버림, company 기사 수는 현재 토픽 이름 기준 경로, `summary.titles`는 줄이기 전 수, 시험 보조 칸 추가가 모두 적혀 있다.
- 테스트 기대값을 바꾸거나 빼서 통과시키지 않았는가: 예. 지운 기대 줄은 이전 토픽 줄 문구 두 곳(`· 통과 M건`·머리줄)과 구조체 글자값에 `scope` 칸을 채운 것뿐이다. 문구 변경은 브리프 "미리 정한 판단"의 통과 건수 줄 모양에서 바로 따라오므로 허용 범위로 본다.
- 제약과 프로젝트 규칙을 지켰는가: 예. 시스템 프롬프트는 `llm_make.py` 270~273줄·`TOPIC_SCHEMA_B` scope 설명과 글자가 같다. `continue_topics` 그대로, 추문을 코드로 거르지 않음, 새 시험 경계값은 설정으로 직접 씀, 실제 API 호출·`docs/` 수정 없음.
- 인계가 실제 변경과 맞는가: 예. 숨긴 자동 항목은 market으로 돌아오면 `sync_topic_lenses`의 `deleted_at` 분기가 되살리고 시험(`company_topic_has_no_lens_…_market_again_restores_lens`)이 확인한다. company 토픽은 고칠 때 항목 검색어 갱신도 건너뛴다(숨긴 항목이라 영향 없음).

## 반영한 것 (통과일 때)
- 영구 문서: DECISIONS 1장에 "17단계 topic-input 구현 세부" 줄(근거: keyword-topic/topic-input). SPEC FR-6 토픽 갱신에 company 토픽 버림·기사 수 세는 법·이전 토픽 줄 `통과 M건`, FR-7 토픽 카드에서 company 제외. STATE 다음 할 일을 `stage-check`로 다시 씀.
- 다음 브리프 확인: `stage-check`의 "줄이기 전·뒤 제목 수" 항목은 결과 줄의 `summary.titles`가 줄이기 전 수뿐이라 "알려진 함정"에 줄인 뒤 수를 세는 법과 company 토픽 보는 곳을 한 줄 더함. 그 밖에는 맞음.
