# llm-request 인계
커밋: 84dd733 | 상태: 완료

## 검사 결과 (실행한 명령과 출력 숫자)
| 명령 | 출력 숫자 |
|---|---|
| cargo test openrouter | 19 passed, 0 failed (새 시험 6개) |
| cargo fmt --check | 통과 |
| cargo clippy --all-targets -- -D warnings | 경고 0 |
| cargo test | 637 passed, 0 failed |

## 브리프에 없던 결정
- 다시 보내기 대상은 전송 단계 실패(연결 오류·시간 초과·본문 읽기 실패)만이다. HTTP 500이나 200+본문 error(둘 다 `Upstream` 종류)는 청구됐을 수 있어 다시 보내지 않는다(브리프는 "Upstream·Timeout"이라 적었으나 "연결 오류·시간 초과" 목표에 맞춤).
- 추론 설정은 `Reasoning` enum(`Off` 기본 / `ModelDefault`=필드 안 보냄)을 `StructuredRequest.reasoning`으로 둠. 호출 3곳은 기본값(끔) 사용.
- `GenerateOk.deepest`는 만든 후보 트리 기준(Jev 자르기 전). 로그는 `깊이 실제/설정`. 로그 문장은 `queue.rs`의 `success_line` 함수로 뺌.
- 기존 시험 헬퍼 `client()`와 `connection_failure_is_upstream`에 `.with_retry_delay_ms(0)`을 붙임(기대값 변경 없음, 5초 대기 방지).
- 범위 밖: rust/ripple/src/server/stocks/match.rs — `StructuredRequest`에 `reasoning: Reasoning::default()` 한 줄 — 새 필드 컴파일 수정
- 범위 밖: rust/ripple/src/server/topics/run.rs — 같은 한 줄 — 같은 이유
- 범위 밖: rust/ripple/src/server/budget/usage_tests.rs — 같은 한 줄 — 같은 이유

## 발견한 것 (함정, 실패 교훈, 기존 코드의 버그)
- 본문 읽기 실패 시험은 wiremock으로 못 만들어 `Content-Length`보다 적게 보내고 끊는 TCP 서버를 시험 안에 둠.
- 실제 API는 부르지 않음. 추론 끄기 200 확인은 paid-compare 기록 그대로.

## 상위로 올린 것
없음

## 다음 작업에 영향
- 키워드 생성·토픽 갱신·종목 찾기 요청은 이제 `"reasoning": {"enabled": false}`를 보낸다.
- 문서 반영 필요(상위): 재시도 범위(전송 단계만), `Reasoning` 칸.
