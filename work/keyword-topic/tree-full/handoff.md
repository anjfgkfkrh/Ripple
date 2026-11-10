# tree-full 인계
커밋: 98ac95d | 상태: 완료

## 검사 결과 (실행한 명령과 출력 숫자)
| 명령 | 출력 숫자 |
|---|---|
| `cargo test --lib` | 672 통과, 0 실패 (keywords 시험 85) |
| `/check` R1 fmt | 오류 0 |
| `/check` R2 clippy -D warnings | 경고 0 |
| `/check` R3 cargo test | 672 통과, 0 실패 |
| `/check` R4 build.sh --release | 오류 0 |
| `/check` pnpm format:check / lint / typecheck | 오류 0 / 0 / 0 |
| `/check` pnpm test | 7 통과, 0 실패 |
| `/check` pnpm test:e2e | 49 통과, 0 실패, 5 건너뜀 |

## 브리프에 없던 결정
- 범위 밖: `rust/ripple/src/test_support.rs` — `sample_keyword_tree`에 `aliases`(1개)·`anchors`(5개) 추가 — 새 형식은 두 칸이 필수라 이 표본을 쓰는 `queue_tests`·`generate_tests`가 깨짐
- 범위 밖: `rust/ripple/src/server/schedule/scheduler.rs` — `KeywordEnqueuer` 구현에서 `KeywordQueue<L, D>`의 `D` 제거 — `KeywordQueue`에서 decider 칸을 없앰
- 범위 밖: `rust/ripple/src/worker/index.rs` — `KeywordQueue::new` 호출에서 decider 인자 뺌(decider 자체는 다른 곳에서 계속 씀) — 같은 이유
- `KeywordQueue`에서 `D`·decider를 없앴다(브리프는 `GenerateOptions.decider`만 말함). `NoDecider`도 삭제.
- 상한은 규칙 검증을 통과한 키워드 기준으로 단계마다 앞에서부터 센다(버린 것은 상한을 채우지 않음). `MAX_CANDIDATES`·`CANDIDATES_PER_LEVEL_*`·`MAX_KEPT_KEYWORDS`·`KeepRule`·`prune_keyword_tree` 삭제, `MAX_DEPTH`는 2.
- `GenerateOk` 칸: `candidates`·`rated`·`unrated_reason`·`headlines`를 없애고 `stat_words`(참고한 통계 낱말 수)·`aliases`·`anchors`(저장한 수)를 더함. 결과 줄은 `키워드 생성: 종목 id N, K개(깊이 실제/설정, 규칙에 안 맞아 버림 …, 별칭 a개·고유어 b개, 통계 낱말 w개 참고), 초`.
- `replace_keywords`는 그대로 두고 `replace_tree(conn, id, tree, terms: Option<(별칭, 고유어)>, at)`를 더했다(`replace_keywords`는 `terms=None`으로 부름). `judge_item`은 `topics/run.rs`가 써서 남김(`topic_summary` 인자는 그대로).
- 통계 낱말: 새 파일 `keywords/stat_words.rs`(+`_tests.rs`). 점수가 같으면 낱말 큰 쪽이 먼저(파이썬 `sort(reverse=True)`와 같음). 배경이 비면 빈 목록. 대상 평가는 `evaluated_at >= now-14일`, 상한 없음.
- `prompts/keywords.json` 형식: `headlines` 칸을 `statWords`(`{articles, words:[[낱말,수],…]}` 또는 null)로 바꿈. 내용은 `llm_make.py`의 `new_full_messages`(BROAD=True)를 파이썬으로 실제 돌려 만든 것(주제 항목은 파이썬에 없어 Rust 쪽 `target_words` 문구를 같은 방식으로 얹음).
- `keyword_tree.json`은 `NEW_SCHEMA`·`KW_ITEM` 그대로라 `level`의 `minimum/maximum`이 없어지고 최상위 설명도 `NEW_SCHEMA`의 것이다.
- E2E 가짜 서버: 8개 트리에서 '공급망'을 뺐다(Jev 평가가 없어져 7개가 되도록). 별칭 `가짜별칭`(어느 제목에도 없음), 고유어 `반도체·데이터센터·HBM·메모리·GPU`. 가짜 Jev의 키워드 약점(`공급망`) 분기도 지움. `keywords.spec.ts`는 점수 확인 줄과 그 주석만 바꿈.
- 시험 고침: `tree_tests`(깊이 3~4·후보 상한·Jev 고르기 시험을 새 규칙으로, 파서 시험의 입력에 `aliases`·`anchors` 추가), `generate_tests`(Jev 시험 4개 삭제, 헤드라인 시험을 통계 낱말 시험으로, 파서 입력 보강), `queue_tests`(`success_line`).

## 발견한 것 (함정, 실패 교훈, 기존 코드의 버그)
- 프롬프트 한글 문법 흠이 파이썬 문구 그대로 들어감("주제과", "요인로"). 글자 그대로 옮기라는 브리프를 따랐다.
- 통계 낱말 계산은 `feed_headlines` 전체를 매번 읽는다(배경). 규모가 커지면 생성 때 느려질 수 있으나 이번 시험 Docker 확인에서 시간은 재지 않았다.

## 상위로 올린 것
없음

## 다음 작업에 영향
- 설정 화면의 깊이 범위(1~4)는 아직 그대로다(tree-screen이 고침). 설정이 3~4여도 생성은 2로 자른다. `read.rs`의 `clamp_depth`도 같은 함수라 읽는 쪽 깊이 비교도 1~2로 잘린다.
- 시험 Docker 확인 때 볼 것: `validate_aliases`가 버린 별칭 수, 통계 낱말 계산 시간.
- `docs/`는 고치지 않았다(SPEC·DECISIONS 반영은 review).
