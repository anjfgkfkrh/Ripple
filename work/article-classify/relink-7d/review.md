# relink-7d 리뷰
결과: 통과
범위 밖 무기록: 0

## 범위
- 바뀐 파일은 브리프 범위 4개(`backfill.rs`·`backfill_tests.rs`·`settings.rs`·`worker/index.rs`)와 기록 파일뿐. untracked `work/metrics.csv`·`work/article-classify/metrics.csv`는 작업 전부터 있던 것(인계 "발견한 것").

## 검사 재실행 (/check)
| 검사 | 인계 | 재실행 |
|---|---|---|
| cargo fmt / clippy | 통과 / 경고 0 | 통과 / 경고 0 |
| cargo test | 741 passed | 741 통과, 0 실패 |
| build.sh --release | 성공 | 통과 |
| pnpm format:check | 실패 1(first-response.json, 기존) | 같음 |
| lint / typecheck / pnpm test | 통과 / 통과 / 7 | 같음 |
| test:e2e | 49 passed, 5 skipped | 49 통과, 5 건너뜀 |

## 확인
- 대상: `published_at >= now - EVALUATION_WINDOW_MS`, `load_match_stocks`(지우지 않은 항목)에서 `kind = Market`만. INSERT 칸과 `ON CONFLICT DO NOTHING`은 `save_items`와 같고 유니크 인덱스 `article_matches_article_stock`(0003)이 중복을 막는다. 기사·평가는 고치지 않고 AI 호출 없음.
- 평가: `pending_evaluations`는 평가 없는 연결을 `articles.published_at` 7일 창으로 고르므로 새 연결이 다음 회차에 평가된다(gated = 0인 별칭 이름 연결).
- 시험이 완료 조건을 모두 다룬다: 7일 안·밖, 요약에서 걸림, 이미 키워드로 걸린 기사 새 줄 없음, 지운 미국 증시 없음, 종목 항목 없음, 표시 뒤 두 번째 실행 없음·0건이어도 표시. 시작에서 부르는 작은 함수 시험 있음. 기존 기대값 변경 없음.
- 브리프에 없던 결정(작은 함수, 직접 INSERT, 이름, created_at = now)은 인계에 있고 모두 작고 되돌리기 쉽다.

## 반영
- DECISIONS.md 1장 "18단계 relink-7d 구현 세부" 줄 추가, 계획 답 ⑩·계획 세부 ⑨에 근거.
- SPEC.md 시장 별칭 문단에 시작 때 한 번 다시 걸기 한 문장.
- STATE.md 다시 씀.
- `stage-check` 브리프 알려진 함정에 표시 설정 한 줄(다음 작업 `stock-subject`는 이번 인계와 무관해 고치지 않음).
