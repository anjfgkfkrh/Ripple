# topic-input 인계
커밋: 26c91a6 | 상태: 완료

## 검사 결과 (실행한 명령과 출력 숫자)
| 명령 | 출력 숫자 |
|---|---|
| cargo fmt --check (fmt 적용 후) | 차이 0 |
| cargo clippy -D warnings | 경고 0 |
| cargo test | 727 통과, 0 실패 |
| /check R4 ./build.sh --release | 오류 0 |
| pnpm format:check / lint / typecheck | 오류 0 / 0 / 0 |
| pnpm test | 7 통과 |
| pnpm test:e2e | 49 통과, 0 실패, 5 건너뜀 |

## 브리프에 없던 결정
- 기대값 변경(브리프 제약 밖이나 새 규칙에서 곧바로 따라옴): `topics_tests.rs`의 `prompt_has_my_items_previous_topics_and_new_titles`와 `topic_ended_within_48_hours_...`의 이전 토픽 줄 기대 글자에 ` · 통과 M건`과 머리줄을 반영(통과 건수 표시 결정에 따름). 통과 기준을 키워드 기준으로 기대하는 기존 시험은 없어서 바꾸지 않음.
- 시험 보조 구조체 칸 추가: `RawTopic`·`RefreshedTopic`에 `scope`, `PreviousTopic`에 `shown`(serde default 0)을 더해 시험의 구조체 글자값과 `raw_topic`·`refreshed`·`Ans` 보조에 칸만 채움.
- `scope` 해석은 빠진 값·글자 아닌 값도 market(응답에 scope가 없는 기존 시험이 그대로 통과). 스키마는 필수.
- company 토픽은 관련 종목이 정리 뒤 비면 이름 중복·id 사용 기록에도 넣지 않고 버림.
- company 토픽 기사 수는 `keywords` 1단계 가지(topic_id)로 붙은 종목을 찾고 경로는 `종목 표시 이름 → 토픽 이름`으로 계산(가지 경로 칸이 아니라 현재 토픽 이름 기준).
- `reduce_titles`는 `topics/refresh.rs`에 둠. `MIN_REFRESH_TITLES`는 줄이기 전 수로 보고 `summary.titles`도 줄이기 전 수.
- `cargo fmt`가 범위 안 파일만 고침.
- 범위 밖 변경 없음(`work/metrics.csv`는 작업 전부터 있던 미추적 파일).

## 발견한 것
- 회귀: `prompts/topics.json`의 시스템 문구는 `llm_make.py`의 `topic_system("B")` 출력으로 그대로 만듦(두 사례 모두). 사용자 문구는 손으로 맞춤.
- 새 시험은 경계값(영향도 6 대 5)을 설정 기본값에 기대지 않고 직접 값으로 씀.

## 상위로 올린 것
없음. 통과 건수는 DECISIONS 1장 "17단계 토픽 통과 건수" 줄이 보이기로 정해 구현함.

## 다음 작업에 영향
- 문서 갱신 필요(`docs/`는 건드리지 않음): AI 응답 형식에 `scope` 추가, company 토픽 처리, `topic_article_counts` 통과 기준 변경, 이전 토픽 줄 모양.
