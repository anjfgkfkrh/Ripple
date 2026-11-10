# eval-merge 인계
커밋: 4e3f1dd | 상태: 완료

## 검사 결과 (실행한 명령과 출력 숫자)
| 명령 | 출력 숫자 |
|---|---|
| `cargo fmt --check` | 통과 |
| `cargo clippy --all-targets -- -D warnings` | 경고 0 |
| `cargo test` | 717 passed, 0 failed (새 시험: questions_tests 2, typesafe_tests 2, evaluate/run_tests 6 + 고친 1) |
| `./build.sh --release` | 성공 |
| `pnpm format:check` | 통과 |
| `pnpm lint` / `pnpm typecheck` | 오류 0 |
| `pnpm test` | 7 passed (1 파일) |
| `pnpm test:e2e` | 49 passed, 5 skipped (모바일 건너뜀) |

## 브리프에 없던 결정
- 고친 기존 시험: `evaluate/run_tests` `market_and_topic_items_put_their_kind_and_topic_summary_into_the_jev_input` — 시장·주제 연결이 `decide`가 아니라 합친 호출로 가므로 `merged_calls`(토픽·시장 항목 증시·설명)를 확인하도록 바꿈. 다른 기존 시험은 그대로.
- `Pending`에 `topic_id: Option<i64>` 추가(같은 토픽의 한국·미국 항목을 토픽 하나로 묶으려고). `pending_evaluations` 질의에 `stocks.topic_id` 열 추가, `gated = 0` 조건은 그대로.
- 새 타입 이름: `MergedInput`·`MergedTopic`·`Scored`·`MarketVerdict`·`MergedDecision`, 함수 `Decider::decide_merged`, `questions::build_merged_state`·`build_merged_questions`·`market_key`.
- 응답에 입력 증시·토픽 키가 빠지면 `bad_format`(Upstream 오류, 형식 오류). 합친 응답이 증시를 빠뜨린 경우(가짜 Decider 등)에도 저장 없이 실패 1건으로 센다.
- 증시 순서는 항상 kr, us. `markets`는 시장·주제 항목이 걸린 증시 전부, `relevance_{m}`은 그 증시의 시장 항목이 걸렸을 때만.
- 토큰 나눔: 입력·출력 토큰을 연결 수로 나누고 나머지는 첫 연결 줄에(묶음 안 순서는 대기 목록 순서).
- 시험용 가짜 `Decider`(`test_support.rs`)에 `merged_calls`, `with_merged(...)`, `fake_merged_decision` 추가. `usage_tests.rs` 가짜에는 `decide_merged`(unreachable)만 더함. `NoDecider`(keywords/generate.rs)는 이 저장소에 없어 손대지 않음.
- 시험 경계값은 새 시험에서 `EVALUATION_MIN_RELEVANCE`·`MIN_MAGNITUDE`를 6으로 직접 씀.
- `e2e/mock-services.mjs`: 합친 요청(`state.markets` 있음)은 키 모양을 엄격히 검사(토픽마다 `relevance_t{n}` score, 증시마다 `magnitude_`·`impact_`, 시장 관련도는 `relevance_{m}`만 추가 허용). 답: 토픽 관련 8점, 시장 항목 관련 3점, 증시 영향도는 토픽이 걸린 기사면 7점·시장 항목만이면 3점. 이전 동작(토픽 강함, 시장 약함)을 이어 갔다.
- 범위 안: `run.rs`에 `store_copy`·`store_evaluation`·`store_stock_decision`·`store_merged_decision` 도우미로 저장을 나눔. 종목 경로 동작은 같음.
- 요약: `evaluated` = 저장한 평가 줄 수, `failed` = 실패한 요청 수, `remaining`은 처리 안 된 연결 수(묶음이 앞서 끝난 연결은 제외).

## 발견한 것 (함정, 실패 교훈, 기존 코드의 버그)
- `serde_json::Map`이 키 순서를 유지한다(preserve_order). 질문 순서(관련도 → 영향도 → 호재/악재)를 시험이 순서까지 확인한다.

## 상위로 올린 것
없음

## 다음 작업에 영향
- 문서(`docs/`)는 고치지 않음. DECISIONS·STATE·SPEC에 "시장·주제 항목은 기사당 한 요청, 사용량은 연결마다 한 줄(토큰 나눔)" 반영이 필요.
- 실측에 없던 시장 항목 관련도 질문(`relevance_{m}`)은 stage-check가 따로 비교해야 함.
