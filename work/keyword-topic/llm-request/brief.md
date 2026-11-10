# llm-request: OpenRouter 실패 분류·재시도·추론 설정, 결과 줄의 깊이
모델: Sonnet / effort: medium | 선행: paid-compare 통과

## 목표
OpenRouter 응답 본문을 읽지 못한 실패를 형식 오류가 아닌 연결 오류로 분류하고(B1), 연결 오류·시간 초과는 한 번 다시 보낸다. 리뷰가 정한 추론 설정을 요청에 넣고, 키워드 결과 줄의 "깊이"를 실제 깊이로 고친다(B2).

## 완료 조건
- `openrouter_tests`에 새 시험 통과: 본문 읽기 실패 → `Upstream`, 연결 오류 뒤 두 번째 성공 → 성공(요청 2번), 시간 초과 두 번 → `Timeout`(요청 2번), 형식 오류(내용이 JSON 아님) → 다시 보내지 않음(요청 1번), 429·402 → 다시 보내지 않음, 추론 설정이 요청 본문에 들어감
- `queue_tests`에 새 시험 통과: 결과 줄의 깊이가 실제 만든 깊이
- `cd rust && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` 통과

## 수정 가능 범위
- `rust/ripple/src/server/ai/llm/`
- `rust/ripple/src/server/ai/llm.rs`
- `rust/ripple/src/server/keywords/queue.rs`
- `rust/ripple/src/server/keywords/queue_tests.rs`
- `rust/ripple/src/server/keywords/generate.rs`(`GenerateOk`에 실제 깊이 칸 더하기만)
- `rust/ripple/src/server/keywords/generate_tests.rs`(위 칸 때문에 깨지는 곳만)

## 볼 위치
- `server/ai/llm/openrouter.rs`의 `generate_json`(본문 읽기 `response.bytes()`, `to_http_error`)
- `server/ai/llm.rs`의 `LlmErrorKind`, `StructuredRequest`
- `server/keywords/queue.rs`의 결과 로그 줄("깊이")
- `docs/DECISIONS.md` 1장 "17단계 `keyword-topic` 계획 답" ⑤⑩, "17단계 계획 때 정한 세부" ⑪과 그 뒤 리뷰가 적은 "17단계 추론 토큰" 줄

## 미리 정한 판단
- 다시 보내기는 `OpenRouterClient` 안에서 한 번만 하고, 대상은 `Upstream`·`Timeout`(본문 읽기 실패 포함)뿐이다. 첫 실패 뒤 기다리는 시간은 필드로 두고 기본 5초, 시험에서는 0으로 둔다.
  이유: 0토큰 실패가 맥이 잠든 시간에 났다. 형식 오류·429·402는 다시 보내도 같거나 비용이 나간다. 버린 대안: `generate_with_budget`에서 재시도(가짜 LLM 시험이 모두 바뀜)
- 본문을 읽다 시간이 다 되면 `Timeout`, 그 밖의 읽기 실패는 `Upstream`(문구 "OpenRouter 응답 본문을 읽지 못했습니다: …"). 성공 상태인데 본문이 JSON이 아니면 지금처럼 형식 오류로 둔다.
  이유: B1은 읽기 실패만의 문제다.
- 추론 설정은 리뷰가 적은 DECISIONS 1장 "17단계 추론 토큰" 줄의 값을 그대로 요청 본문에 넣는다. 그 줄이 "끄지 않음"이면 이 부분은 하지 않는다. 값은 `StructuredRequest`의 선택 칸으로 받아 호출하는 쪽(키워드 생성·토픽 갱신·종목 찾기)마다 줄 수 있게 하고, 기본값은 그 줄이 정한 대로 한다.
  이유: 측정 결과에 따라 상위가 정한 값이다.
- 결과 줄의 깊이: `GenerateOk`에 실제 만든 트리의 가장 깊은 단계를 담는 칸을 더하고, 로그는 그 값을 쓴다. 설정 깊이는 "깊이 2/3"처럼 같이 보인다(`실제/설정`).
  이유: B2(DECISIONS 2장 "키워드·채점 점검").

## 체크포인트 (각각 검사 통과 후 커밋)
1. 본문 읽기 실패 분류와 한 번 다시 보내기 → `openrouter_tests` 새 시험 통과
2. 추론 설정 칸(결정이 "끄지 않음"이면 건너뜀) → `openrouter_tests` 요청 본문 시험 통과
3. 결과 줄의 실제 깊이 → `queue_tests` 새 시험 통과

## 제약
- 기존 시험의 기대값은 바꾸지 않는다(새 칸 때문에 생기는 컴파일 수정만).
- 실제 API를 부르지 않는다(wiremock 등 기존 시험 방식).
- `docs/`는 고치지 않는다(인계에 적음).

## 알려진 함정
- reqwest의 `bytes()` 오류에서 시간 초과는 `is_timeout()`으로 가린다.
- 다시 보낼 때 같은 요청 본문을 다시 만들어야 한다(본문은 한 번 쓰면 사라짐).
- 리뷰(2026-11-10, paid-compare): 값은 `"reasoning": {"enabled": false}`(요청 본문 최상위, `provider`와 나란히). `reasoning.max_tokens`는 추론을 켜는 값이니 쓰지 않는다. `provider.require_parameters: true`와 함께 200이 실제로 확인됨.

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고) + "17단계 추론 토큰" 줄이 없거나 값이 애매할 때
