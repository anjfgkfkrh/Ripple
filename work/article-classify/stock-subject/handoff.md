# article-classify/stock-subject 인계
커밋: 108a3c4 | 상태: 완료

## 검사 결과 (실행한 명령과 출력 숫자)
| 명령 | 출력 숫자 |
|---|---|
| `cargo test questions` (체크포인트 1) | 14 통과, 0 실패 |
| `cargo test typesafe` (체크포인트 2) | 19 통과, 0 실패 |
| `cargo test` (체크포인트 3) | 746 통과, 0 실패 |
| `/check` R1 fmt / R2 clippy | 오류 0 / 경고 0 |
| `/check` R3 cargo test / R4 build --release | 746 통과 0 실패 / 오류 0 |
| `/check` pnpm format·lint·typecheck | 오류 0 |
| `/check` pnpm test | 7 통과, 0 실패 |
| `/check` pnpm test:e2e | 49 통과, 0 실패, 5 건너뜀(모바일 프로젝트) |

## 브리프에 없던 결정
- `store_evaluation`에 인자 `bystander: Option<f64>`를 더했다(종목 평가는 `decision.bystander`, 합친 평가는 `None`). `Evaluation` 구조체와 `find_same_news_evaluation` SELECT, `store_copy` INSERT에도 `bystander` 열을 더했다.
- `typesafe.rs`: 종목 대상만 `answers.bystander.noul`을 읽고(없거나 숫자 아니면 `Upstream` 형식 오류), 시장·주제 대상은 `None`. 질문 글자는 `questions.rs`의 `pub const BYSTANDER_INSTRUCTIONS`.
- `test_support.rs`의 `fake_decision()` 기본 `bystander`는 `Some(0.1)`.
- E2E 가짜 서버: 종목 요청(`state.stock` 있음)은 `bystander.type === "noul"`과 instructions 글자가 있어야 하고, 시장·주제 요청에 `bystander`가 있으면 422. 기본 답은 `noul: 0.1`.
- 새 시험: `questions_tests`(순서·글자·시장/주제 없음), `typesafe_tests`(형식 오류 3종, 시장 대상 None), `run_tests`(저장·복사·NULL 복사·합친 평가 NULL).
- `rust/ripple/src/server/ai/decider/questions.rs` 모듈 설명을 지시대로 고쳤다.

## 발견한 것 (함정, 실패 교훈, 기존 코드의 버그)
- 범위 밖 변경 없음(`git status`에서 작업 전부터 미추적인 `work/metrics.csv`·`work/article-classify/metrics.csv`만 보임).

## 상위로 올린 것
없음

## 다음 작업에 영향
- 종목 평가에서 `bystander` 응답이 없으면 그 평가 전체가 형식 오류로 실패한다(저장하지 않고 다음 회차에 재시도).
