# llm-request 리뷰
결과: 통과
범위 밖 무기록: 0

## 범위 확인
- 검사 명령 재실행: 인계 숫자와 일치(fmt 통과, clippy 경고 0, cargo test 637 passed 0 failed)
- 범위 밖 판단:
  - rust/ripple/src/server/stocks/match.rs: 브리프 빈틈 — `StructuredRequest.reasoning` 새 칸 때문의 한 줄 컴파일 수정(브리프가 호출 세 곳에 칸을 주라 했으나 범위에 안 넣음)
  - rust/ripple/src/server/topics/run.rs: 브리프 빈틈 — 같은 한 줄
  - rust/ripple/src/server/budget/usage_tests.rs: 브리프 빈틈 — 같은 한 줄(시험 헬퍼)

## Opus 확인
- 브리프에 없던 결정이 인계에 빠짐없이 있는가: 예 — 재시도 대상을 전송 단계로 좁힘, `Reasoning` enum, `deepest` 후보 기준, `success_line` 분리, 기존 헬퍼에 `with_retry_delay_ms(0)`. 재시도 좁힘은 계획 답 ⑤("연결 오류·시간 초과")와 맞고 HTTP 500 재청구를 피하므로 인정
- 테스트 기대값을 바꾸거나 빼서 통과시키지 않았는가: 예 — 기존 시험은 새 칸·대기 0만 더함, 단언 변경 없음
- 제약과 프로젝트 규칙을 지켰는가: 예 — 실제 API 안 부름(wiremock·시험 안 TCP 서버), `docs/` 안 고침
- 인계가 실제 변경과 맞는가: 예 — diff와 일치. 새 시험 6개(본문 읽기 실패 2번 요청, 연결 실패 뒤 성공, 시간 초과 2번, 형식 오류 1번, 429·402 1번, 추론 칸)와 queue 시험 1개 확인

## 반영한 것 (통과일 때)
- 영구 문서: DECISIONS 1장에 "17단계 llm-request 구현 세부" 줄 추가(근거 붙임), 2장 B1~B3 줄에 B1·B2 고침 표시, STATE 다음 할 일 갱신
- 다음 브리프 확인: `anchor-schema`는 영향 없음. `tree-full`(대기) 알려진 함정에 `success_line`·`deepest`·새 `StructuredRequest`의 `reasoning` 칸 한 줄 추가
