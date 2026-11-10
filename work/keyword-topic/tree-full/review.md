# tree-full 리뷰
결과: 통과
범위 밖 무기록: 0

## 범위 확인
- 검사 명령을 다시 돌렸고 인계 숫자와 같다: fmt 0, clippy 0, cargo test 672 통과·0 실패, build.sh --release 0, pnpm format/lint/typecheck 0, pnpm test 7 통과, E2E 49 통과·5 건너뜀.
- 범위 밖 판단:
  - `rust/ripple/src/test_support.rs`: 브리프 빈틈. `sample_keyword_tree`에 aliases·anchors를 넣었다. 새 형식에서는 두 칸이 필수다.
  - `rust/ripple/src/server/schedule/scheduler.rs`: 브리프 빈틈. `KeywordQueue`에서 decider 타입 인자를 뺐다. Jev 후보 평가를 없앴으니 따라 바뀌는 곳이다.
  - `rust/ripple/src/worker/index.rs`: 브리프 빈틈. 위와 같은 이유로 `KeywordQueue::new` 인자를 뺐다. 기사·토픽 평가에 쓰는 decider는 그대로다.

## Opus 확인
- 브리프에 없던 결정이 인계에 빠짐없이 있는가: 예. 적힌 것은 decider 칸 제거, 상한을 세는 기준, `GenerateOk` 칸, `replace_tree` 추가, 통계 낱말이 같은 점수일 때의 순서, fixture 형식, E2E 가짜 서버 변경이다.
- 테스트 기대값을 바꾸거나 빼서 통과시키지 않았는가: 예.
  - generate_tests에서 지운 시험은 Jev 평가 4개와 headlines 시험이다. headlines 시험은 통계 낱말 시험으로 바꿨다. 깊이 시험은 1~2 규칙으로 고쳤다. 모두 허용된 범위다.
  - queue_tests `success_line`은 llm-request 리뷰가 고치라고 했던 시험이다. 오히려 `assert_eq`로 더 엄격해졌다.
  - E2E는 점수 확인 줄만 뺐다. "키워드 7개"·'공급망' 0개 기대값은 그대로다. 가짜 AI가 '공급망'을 아예 내지 않게 해서 맞췄다. Jev가 없어졌으니 같은 결과를 내는 정당한 방법이다.
- 제약과 프로젝트 규칙을 지켰는가: 예.
  - 프롬프트는 `new_full_messages`(BROAD=True)와 글자 단위로 같다. 깊이 1 문구는 브리프대로다.
  - 응답 형식은 `NEW_SCHEMA`·`KW_ITEM`과 같다.
  - `toks`·`stat_words`는 파이썬과 같은 정규식·문턱·점수로 옮겼다. 통과 기준은 브리프대로 `meets_display_rule`이다.
  - 고유어가 0개면 저장 전에 실패한다. 별칭·고유어는 키워드와 한 트랜잭션에 넣는다.
  - 최대 출력 토큰은 8,000이다. 실제 API는 부르지 않았고 `docs/`는 고치지 않았다.
- 인계가 실제 변경과 맞는가: 예.

## 남기는 말
- 작은 비효율이 두 가지 있다. 지금 규모에서는 문제가 아니다.
  - `stat_words`는 평가 줄마다 `meets_display_rule`로 설정을 다시 읽는다.
  - 생성 때마다 `feed_headlines` 전체를 배경으로 읽는다. 시간은 stage-check에서 잰다.

## 반영한 것 (통과일 때)
- 영구 문서:
  - DECISIONS 1장: "17단계 tree-full 구현 세부" 줄을 더했다(근거 포함). "11단계 트리 크기 안전 상한"과 "11단계 Jev 점수 기반 설계"(키워드 트리 부분)에 대체됨 표시를 붙였다.
  - SPEC FR-3: 17단계 하위 항목에 한 줄을 더했다(새 키워드 점수 없음, 고유어 0개면 실패, 시장·주제는 별칭·고유어 없음, 깊이 1).
  - STATE: 다시 썼다.
- 다음 브리프 확인:
  - tree-weekly: 수정 가능 범위에 `test_support.rs`(표본·도우미만)를 더했다. 볼 위치에 `replace_tree`·`refine_anchors`·`stat_words.rs`와 1장 줄을 더했다. 알려진 함정에 한 줄을 더했다.
  - tree-screen(대기): 알려진 함정에 `clamp_depth` 1~2와 점수 NULL, E2E 가짜 트리에 관한 한 줄을 더했다.
