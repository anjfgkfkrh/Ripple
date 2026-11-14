# article-nature 리뷰
결과: 통과
범위 밖 무기록: 0

## 범위 확인
- 검사 명령 재실행: 인계 숫자와 일치(`cargo fmt --check` 통과, clippy 경고 0, `cargo test` 768 passed 0 failed, `pnpm format:check`·`lint`·`typecheck` 통과, `pnpm test` 7 passed, `pnpm test:e2e` 49 passed 5 skipped)
- 범위 밖 판단:
  - `rust/ripple/src/server/collect/run.rs`: 브리프 빈틈 — 브리프가 "회차 결과 로그 한 줄에도 붙인다"고 했는데 그 로그가 이 파일에만 있다. 바뀐 것은 로그 형식 문자열에 세 숫자를 더한 것뿐

## Opus 확인
- 브리프에 없던 결정이 인계에 빠짐없이 있는가: 예 — 성격 숨김을 자리표 대신 글자로 넣음, 평가 note가 없을 때만 성격 묻기, 성격 쪽 연속 실패 멈춤, JSON 칸은 0보다 클 때만, `add_link` 도우미 변경, `NoDecider` 없음, 복사 후보 순서가 모두 인계에 있고 diff와 맞다
- 테스트 기대값을 바꾸거나 빼서 통과시키지 않았는가: 예 — 기존 시험에서 지운 줄은 `add_link`의 INSERT 문(기사를 `nature_at` 있는 것으로 넣도록)과 import 한 줄뿐. 기대값은 그대로, `EvaluationSummary` 리터럴에 새 칸 0만 더함
- 제약과 프로젝트 규칙을 지켰는가: 예 — `decide`·`decide_merged`의 질문·저장은 그대로, 7일 창(`EVALUATION_WINDOW_MS`) 밖은 묻지 않음, 호출 전 `assert_within_budget`·호출 뒤 `record_usage`
- 인계가 실제 변경과 맞는가: 예 — 질문 글자·순서·`noul` 모양, 6확률 해석과 형식 오류, 대상 고르기(성격 부분 제외), 같은 소식 복사, 사용량 한 줄, 요약 세 칸, 숨김 조각 네 곳과 `INNER JOIN articles` 세 곳, 가짜 TypeSafe 규칙을 diff로 확인

## 반영한 것 (통과일 때)
- 영구 문서: DECISIONS.md 1장에 "18단계 article-nature 구현 세부" 줄(근거: article-classify/article-nature). SPEC.md FR-5에 "기사 성격(18단계)" 항목. STATE.md 다시 씀(다음은 `nature-tabs`, measure 질문 줄은 문구 그대로 구현됐다고 고침)
- 다음 브리프 확인: `nature-tabs`는 인계 "다음 작업에 영향"과 맞다(`classify_nature`·`NatureTag::label()`, 가짜 답 규칙). 알려진 함정에 한 줄(`display_rule`에 이미 성격 숨김이 자리표 없이 붙어 있음, "[포토]" E2E는 그 기사가 평가 기준을 넘어야 의미 있음). `stage-check` 알려진 함정에 한 줄(성격 묻기는 평가 루프가 멈추지 않은 회차에만 돌아 평가 대기가 많으면 첫 회차들은 묻지 않음)
- 남길 말: 평가 대기가 매 회차 한도를 채우면 성격 묻기가 계속 밀린다(호출 수를 합쳐 세니 브리프 의도와 같음, 실제 규모는 stage-check에서 본다). 상태 화면은 새 요약 칸을 읽지 않는다(브리프 요구 밖)
