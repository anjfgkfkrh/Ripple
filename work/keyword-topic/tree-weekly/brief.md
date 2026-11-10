# tree-weekly: 키워드 실적과 정기 증분 점검
모델: Sonnet / effort: medium | 선행: tree-full 통과

## 목표
정기 점검 때 트리를 통째로 다시 만들지 않는다. 지금 트리·고유어·별칭을 주고 뺄 것·더할 것만 받아 반영한다. 뺄 것은 최근 4주 통과 실적으로 지키고, 상한을 넘으면 마지막 통과가 오래된 것부터 밀어낸다. 점검은 매일 06:00에 확인해 이번 주기 전에 만든 트리만 고르므로, 실패한 항목은 다음 날 다시 한다.

## 완료 조건
- 키워드 실적 시험 통과: 같은 종목에서 경로의 마지막 낱말이 같은 연결을 셈(전체 재생성 뒤에도 이어짐), 걸러진(`gated`) 연결도 "걸린 수"에 셈, 통과는 화면 표시 기준, 기간 밖은 안 셈, 마지막 통과 시각
- `generate_tests` 새 시험 통과(증분):
  - 뺄 것이라도 최근 4주 통과가 있으면 남음
  - 1단계를 빼면 그 아래도 빠짐
  - 더할 것은 최대 3개이고 검증(부모·중복·대상 이름·시장 금지어)을 거침
  - 바뀌지 않은 키워드는 id·만든 시각이 그대로임
  - 1단계 6·2단계 12를 넘으면 마지막 통과(없으면 만든 시각)가 가장 오래된 것부터 빠지고 토픽 가지는 세지 않음
  - 고유어 빼기·더하기(최대 3개)와 별칭 더하기가 검증을 거침
  - 바뀐 것이 없어도 `keywords_generated_at`이 갱신됨
  - 실패하면 트리를 그대로 두고 실패 상태
- `plan_tests` 새 시험 통과: 하루 1회·주 1회 모두 매일 06:00에 확인하고, 대상은 주기 시작(하루: 오늘 06:00, 주: 이번 주 월요일 06:00) 전에 만든 항목. 실패한 항목은 다음 날 06:00에 다시 고름. 대기·만드는 중인 항목은 고르지 않음
- `queue_tests` 새 시험 통과: 등록·"다시 만들기"·대기 중 이어 만들기는 전체 생성, 정기 점검은 트리가 있으면 증분이고 트리가 없거나 종목의 고유어가 비면 전체 생성
- `/check` 전체 통과

## 수정 가능 범위
- `rust/ripple/src/server/keywords/`
- `rust/ripple/src/server/prompts/`(새 고정 파일 `keyword_update.json`)
- `rust/ripple/src/server/ai/llm/schemas/`(새 파일 `keyword_update.json`)
- `rust/ripple/src/server/ai/llm/schemas.rs`
- `rust/ripple/src/server/ai/llm/schemas_tests.rs`
- `rust/ripple/src/server/schedule/`
- `rust/ripple/src/worker/`(대기열 호출 모양이 바뀌는 곳만)
- `e2e/mock-services.mjs`(`keyword_update` 답 더하기)
- `rust/ripple/src/test_support.rs`(증분 시험에 쓸 표본·도우미만)

## 볼 위치
- `scripts/analysis/keyword-topic/llm_make.py`: `UPDATE_SCHEMA`, `update_messages`(문구 그대로), `stat_words`
- `server/keywords/generate.rs`(`replace_tree`·`try_generate`), `server/keywords/tree.rs`(`build_keyword_tree`·`refine_anchors`·`LEVEL1_MAX`·`LEVEL2_MAX`), `server/keywords/stat_words.rs`(`stat_words`), `server/keywords/queue.rs`(`success_line`)
- `docs/DECISIONS.md` 1장 "17단계 tree-full 구현 세부"
- `server/schedule/plan.rs`의 `keyword_plan`, `last_seoul_boundary`·`next_seoul_boundary`, `server/schedule/scheduler.rs`
- `server/stocks/aliases.rs`의 `validate_aliases`
- `docs/DECISIONS.md` 1장 "키워드 트리·토픽 고도화 세부" ②③④, "17단계 `keyword-topic` 계획 답" ③⑤, "17단계 계획 때 정한 세부" ①②③④

## 미리 정한 판단
- 프롬프트는 `update_messages`, 응답 형식은 `UPDATE_SCHEMA`를 글자 그대로 옮긴다(스키마 이름 `keyword_update`). 사용자 메시지의 지금 트리는 구조 키워드(토픽 가지 빼고)만 `- 1단계 > 2단계, 2단계` 모양이다. 종목은 고유어·별칭·통계 낱말(tree-full의 계산)을 넣는다. 최대 출력 토큰 6,000.
  이유: 실측 문구다(출력 175~1,573토큰). 버린 대안: 전체 재생성(겹침 2/23~9/15로 흔들림)
- 키워드 실적 함수(이름 자유): (종목, 기준 시각) → 키워드 글자마다 걸린 수·통과 수·마지막 통과 시각. `article_matches.match_kind = 'keyword'`이고 `keyword_path`를 `" → "`로 나눈 마지막 낱말이 키워드 글자인 연결을 센다. 통과는 평가가 화면 표시 기준을 넘는 것(`meets_display_rule`과 같은 규칙)이다. 정기 점검은 최근 28일로 부른다.
  이유: 세부 ②③. 전체 재생성 뒤에도 실적이 이어진다(결정 ⑤).
- 반영 순서(한 트랜잭션):
  1. 뺄 것: 정규화한 글자로 찾는다. 28일 안에 통과가 있으면 남긴다.
  2. 더할 것: 최대 3개. 2단계의 부모는 남은 1단계나 이번에 더한 1단계여야 한다.
  3. 상한: 1단계 6·2단계 12. 넘으면 "마지막 통과 시각, 없으면 만든 시각"이 가장 오래된 것부터 뺀다(1단계를 빼면 그 아래도).
  4. 고유어: 빼기 다음 더하기(최대 3개, tree-full과 같은 다듬기). 고유어가 0개가 되면 빼기를 하지 않는다.
  5. 별칭: 더할 것만 `validate_aliases`.
  6. 바뀌지 않은 줄은 그대로 두고, 상태 `ready`, `keywords_generated_at = 지금`.
  이유: 세부 ③④, 계획 세부 ①④. 버린 대안: 통과 0이면 제외(실측 통과 6/65 놓침)
- 정기 점검 시각: 설정 주기와 상관없이 매일 06:00(한국 시간)에 계획을 확인한다. 대상은 `keywords_generated_at`이 주기 시작보다 이른 항목이다(하루: 오늘 06:00, 주: 이번 주 월요일 06:00, 실패 상태 포함). 꺼져 있었으면 켜진 뒤 처음 확인 때 한 번(지금 규칙 그대로).
  이유: 사용자 답 5A(실패하면 다음 날 다시).
- 대기열은 항목마다 방식(전체·정기)을 들고 다닌다. 정기는 실행할 때 트리(구조 키워드)가 없거나 종목의 고유어가 비면 전체 생성으로 바꾼다. 같은 항목이 이미 대기 중이면 지금처럼 무시하되, 전체 생성이 정기를 덮는다.
- E2E 가짜 서버의 `keyword_update` 답은 빈 목록(바뀐 것 없음)이다.

## 체크포인트 (각각 검사 통과 후 커밋)
1. 키워드 실적 함수 → 실적 시험 통과
2. 증분 프롬프트·형식·반영 → `generate_tests` 증분 시험, `schemas_tests` 통과
3. 대기열 방식과 매일 06:00 계획 → `queue_tests`·`plan_tests` 새 시험 통과
4. E2E 가짜 서버 → `/check` 전체 통과

## 제약
- 바꿔도 되는 기대값은 `plan_tests`·`scheduler_tests`에서 "주기마다 한 번"·주 1회 월요일 확인 시각을 다루는 시험(새 규칙으로 고침)뿐이다. 그 밖의 기대값은 바꾸지 않는다.
- 설정 기본값(주 1회)·설정 화면은 tree-screen이 고친다.
- 실제 API를 부르지 않는다. `docs/`는 고치지 않는다(인계에 적음).

## 알려진 함정
- 실측에서 주간 점검은 매번 2~3개를 더했다. 상한·밀어내기가 없으면 트리가 계속 커진다.
- 0011 이전의 옛 트리는 40개까지 있고 깊이 3이다. 첫 정기 점검에서 3단계 이상은 빼고(상한 규칙으로) 1·2단계만 상한까지 남긴다.
- 리뷰(2026-11-10, tree-full): 새 키워드는 `relevance`·`magnitude`가 NULL이고 `KeywordQueue`에는 decider가 없다(`KeywordQueue<L>`, `new(db, llm, now)`). 고유어 다듬기는 `tree.rs` `refine_anchors`, 키워드·별칭·고유어 한 트랜잭션 저장은 `generate.rs` `replace_tree`를 다시 쓴다. 전체 생성은 고유어 0개면 `NoAnchors`로 실패한다. 통계 낱말은 `feed_headlines` 전체를 읽으므로 정기 점검에서 항목마다 부르면 그만큼 읽는다. 결과 줄(`success_line`)에 정기 점검 줄을 따로 만들면 `queue_tests`의 `success_line` 시험 기대값은 그대로 둔다.

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고)
