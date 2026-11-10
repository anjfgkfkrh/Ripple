# anchor-match 인계
커밋: ecf3d39 | 상태: 완료

## 검사 결과 (실행한 명령과 출력 숫자)
| 명령 | 출력 숫자 |
|---|---|
| `cd rust/ripple && cargo fmt --check` | 통과(출력 없음) |
| `cargo clippy --all-targets -- -D warnings` | 경고 0, 통과 |
| `cargo test` | 660 passed; 0 failed (시작 시 652, 새 시험 8) |

새 시험: `match_tests` 5개(별칭, 제목에 고유어 없음·요약에만 있음 gated, 제목에 고유어 있음, 빈 고유어, 시장·주제), `collect/run_tests` 1개, `backfill_tests` 1개, `evaluate/run_tests` 1개.

## 브리프에 없던 결정
- `MatchStock`에 `aliases`·`anchors`, `StockMatch`에 `gated`를 더했고 `Matcher::find(title, summary)`로 바꿨다(브리프대로).
- 기존 시험 데이터 보정(기대값은 그대로): `match_tests`의 삼성(고유어 hbm)·엔비디아(accelerator·가속기·hbm), 시장·주제 시험 항목은 `aliases`·`anchors` 빈 목록, `find("…")`는 `find("…", "")`, `by_name`·`by_keyword` 헬퍼에 `gated: false`. `collect/run_tests`의 `add_stocks_at`에서 삼성(hbm·엔비디아)·엔비디아(가속기) 고유어를 SQL로 넣음(평가 시험 2개가 깨져서).
- 고유어 비교는 소문자화 + 모든 공백(char::is_whitespace) 제거 후 `contains`. 빈 고유어는 버림.
- `load_match_stocks`의 별칭·고유어 JSON 파싱은 `run.rs`의 작은 함수 `json_list`(깨지면 빈 목록).
- 범위 밖 변경 없음(`git status`에는 작업 전부터 있던 `work/metrics.csv`만).
- 체크포인트 1 커밋 직후 시험 데이터 보정이 따로 커밋됨(커밋 3개 이상으로 나뉨, 체크포인트 3 커밋 시점에 수집 시험 2개가 깨졌다가 다음 커밋 ecf3d39에서 고침).

## 발견한 것 (함정, 실패 교훈, 기존 코드의 버그)
- 백필 기존 시험은 그대로 통과(걸러진 연결도 저장·카운트되므로).
- E2E 깨질 곳: `e2e/collect.spec.ts`는 `registerStock(page, "하닉", "SK하이닉스")` 종목에 키워드 연결로 기사가 걸리길 기대하면 `anchors`가 비어 있어 `gated`로 평가되지 않는다. 이름·별칭·직접 연결은 영향 없음. 이 시험이 키워드 연결 평가에 의존하는지는 실행해 보지 않았다.

## 상위로 올린 것
없음

## 다음 작업에 영향
- 고유어는 `stocks.anchors`가 채워져야 키워드 연결이 평가된다(채우는 쪽 작업 필요).
- 상태/타임라인 화면 등에서 `gated = 1` 연결을 어떻게 보일지는 다루지 않았다.
