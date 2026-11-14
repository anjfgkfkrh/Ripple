# article-classify/article-nature 인계
커밋: 7f7df1f | 상태: 완료

> 넣지 않는 것: 코드 설명(커밋이 기억함), 작업 경위, 고생한 과정.

## 검사 결과 (실행한 명령과 출력 숫자)
| 명령 | 출력 숫자 |
|---|---|
| `cargo test --lib decider`(체크포인트 1) | 35 passed, 0 failed |
| `cargo test --lib evaluate::run`(체크포인트 2) | 30 passed, 0 failed |
| `cargo fmt --check` | 오류 0 |
| `cargo clippy --all-targets -- -D warnings` | 경고 0 |
| `cargo test` | 768 passed, 0 failed |
| `./build.sh --release` | 성공 |
| `pnpm format:check` / `pnpm lint` / `pnpm typecheck` | 통과(format은 `mock-services.mjs`를 prettier로 고친 뒤) / 오류 0 / 오류 0 |
| `pnpm test` | 7 passed |
| `pnpm test:e2e` | 49 passed, 5 skipped(모바일 건너뜀 설계) |

## 브리프에 없던 결정
- 범위 밖: `rust/ripple/src/server/collect/run.rs` — 평가 회차 로그 한 줄에 `성격 물음 N·복사 N·실패 N` 추가 — 브리프가 "회차 결과 로그 한 줄에 붙인다"고 했는데 그 로그가 이 파일에만 있고 수정 가능 범위에 없음
- 숨김 SQL의 성격 기준은 `?` 자리표가 아니라 `not_nature_hidden_sql()`(`evaluate/run.rs`)이 `NATURE_THRESHOLD` 값을 글자에 바로 넣는다(자리표 번호를 건드리지 않으려고). 쓰는 곳: `timeline/read.rs` `display_rule`, `topics/store.rs` `topic_article_counts`, `keywords/performance.rs`, `keywords/stat_words.rs`. 카드 항목 줄·붙은 종목 조회(`read.rs`)에는 `INNER JOIN articles`를 더했다.
- 성격 묻기 코드는 `evaluate/run.rs` 안(`classify_pending_natures` 등)에 뒀다. `evaluate.rs`(모듈 선언)는 수정 가능 범위 밖이라 새 파일을 만들지 않았다.
- 성격 묻기는 평가 루프가 멈춘 이유(note)가 없을 때만 돈다(키 오류·예산·연달아 실패·회차 한도). 성격 쪽 멈춤 note는 평가 note가 없을 때만 쓴다. 한도 문구는 "성격 묻기는 회차 한도 N건에 닿아 …"로 평가 note의 "회차 한도"로 시작하는 줄과 구분했다(그 줄은 남은 평가 수를 붙임).
- 성격 쪽도 연달아 3번 실패하면 이번 회차 성격 묻기를 멈춘다(`MAX_CONSECUTIVE_FAILURES` 재사용). 요약 `failed`에는 더하지 않고 `nature_failed`만 센다.
- `EvaluationSummary.to_json`의 `natureAsked`·`natureCopied`·`natureFailed`는 0보다 클 때만 쓴다(기존 JSON·기존 시험 문자열 유지). 요약 구조체에는 serde가 없어 "serde 기본 0"은 해당 없음. `collect/read.rs`(JSON → 읽기 모델)와 `web/status.rs`는 새 칸을 읽지 않는다(범위 밖이라 손대지 않음).
- 기존 시험 변경: `run_tests.rs`의 `add_link` 도우미가 기사를 `nature_at`이 이미 있는 것으로 넣는다(기존 평가 시험의 사용량·비용·개수가 성격 묻기에 흔들리지 않게, 기대값은 그대로). `EvaluationSummary` 리터럴에 새 세 칸 `0` 추가. 새 성격 시험은 `insert_link`(성격 NULL)를 쓴다.
- 브리프의 `generate.rs`의 `NoDecider`는 코드에 없다(Decider 구현은 TypeSafe·`test_support.rs`의 FakeDecider·`usage_tests.rs`의 FakeDecider뿐). `generate.rs`는 안 고쳤다. `usage_tests.rs` FakeDecider에 `classify_nature`(unreachable)를 더했다.
- 같은 소식 복사의 후보는 발행 ±24시간·`nature_at` 있는 기사 중 발행 최근순 첫 `is_same_news`다. 복사한 기사의 `nature_at`은 복사 시각이다.
- 성격 비용·토큰은 응답 모델명으로 `usage_logs`에 한 줄 남긴다(provider typesafe).

## 발견한 것 (함정, 실패 교훈, 기존 코드의 버그)
- 없음

## 상위로 올린 것
- 없음

## 다음 작업에 영향
- `nature-tabs`: 타임라인 쪽에서 성격 태그를 보일 때 `articles.nature_*`를 `classify_nature`(schema.rs)로 읽는다. E2E 가짜 TypeSafe는 `[포토]`→media 0.9(숨김), `전망`→forecast 0.9·fact 0.2, 그 밖→fact 0.9, 나머지 0.1로 답한다. 시험용 worker가 평가 직후 같은 회차에 성격을 채운다.
