# eval-merge 리뷰
결과: 통과
범위 밖 무기록: 0

## 범위 확인
- 검사 명령 재실행: 인계 숫자와 일치(cargo fmt·clippy 통과, cargo test 717 통과 0 실패, release 빌드 성공, pnpm format·lint·typecheck 오류 0, pnpm test 7 통과, E2E 49 통과·5 건너뜀)
- 범위 밖 판단:
  - 없음(변경 파일 모두 수정 가능 범위 안. `work/keyword-topic/metrics.csv`는 run의 기록 파일)

## Opus 확인
- 브리프에 없던 결정이 인계에 빠짐없이 있는가: 예 — `Pending.topic_id`, 타입·함수 이름, 증시 순서 kr·us, 빠진 키는 형식 오류, 토큰 나머지 첫 줄, 요약 `remaining` 셈, E2E 가짜 점수, 저장 도우미 분리 모두 인계에 있음
- 테스트 기대값을 바꾸거나 빼서 통과시키지 않았는가: 예 — 고친 기존 시험은 `market_and_topic_items_put_their_kind_and_topic_summary_into_the_jev_input` 하나로, 브리프가 허용한 "시장·주제 연결을 하나씩 묻는다고 기대하는 것"이며 같은 내용(종류·토픽 설명)을 합친 호출 입력에서 확인함. fixture·다른 기존 기대값 그대로
- 제약과 프로젝트 규칙을 지켰는가: 예 — 실제 API 호출 없음, `docs/` 안 고침, `gated = 0` 유지, 기존 `decide`·`rate` 그대로, 예산 검사는 호출 전에 요청 1번으로, 새 시험 경계값은 설정으로 직접 씀
- 인계가 실제 변경과 맞는가: 예 — diff로 질문 모양·순서(questions.rs), 키별 해석·형식 오류(typesafe.rs), 묶기·복사·토큰 나눔·실패 시 남김(run.rs)을 확인

## 반려 사유 / 수정 지시 (반려일 때)
- 없음

## 반영한 것 (통과일 때)
- 영구 문서: DECISIONS 1장 "17단계 eval-merge 구현 세부" 한 줄(근거: keyword-topic/eval-merge), SPEC 6.3 Jev 연동에 기사당 한 요청 한 문장, STATE 다시 씀(eval-merge 통과, 다음 topic-input)
- 다음 브리프 확인: topic-input은 이번 변경과 무관해 그대로. stage-check "알려진 함정"에 `usage_logs` 줄 수 ≠ Jev 호출 수, 시장 항목 관련도는 실측에 없던 질문이라는 줄을 더함
- 참고(작음, 고치지 않음): `store_merged_decision`이 false를 돌리는 경우(응답에 증시가 빠짐)는 Jev 호출이 있었는데 사용량을 남기지 않는다. 실제 `TypeSafeDecider`는 키가 빠지면 먼저 형식 오류를 내므로 가짜 Decider에서만 생긴다
