# tree-screen 보고 (시도 1)

> 작성: run(메인 세션). worker가 Write 거부(하네스 규칙: 서브에이전트는 보고 파일을 쓰지 못함)로 못 써서 worker가 보낸 보고 텍스트를 그대로 옮김. 판단 없음. 마지막 커밋 f968207, 브랜치 af/keyword-topic.

상태: 보고함

## 멈춘 이유
기대값 변경 필요 + 범위 밖 파일. 브리프가 정한 기본값(`KEYWORDS_REGENERATE_EVERY` daily→weekly, `EVALUATION_MIN_MAGNITUDE` 6→5)을 `rust/ripple/src/server/settings.rs`에 적용하자 "수정 가능 범위" 밖의 기존 시험 12개가 실패한다. 브리프 "제약"은 바꿔도 되는 기대값을 `e2e/settings.spec.ts`, `e2e/keywords.spec.ts`, `settings_tests`·`settings_form_tests`로 한정한다. 아래 파일은 고치지 않았다.

## 재현
`cd rust/ripple && cargo test` → `695 passed; 12 failed`

| 시험 | 파일 | 어긋남(실제 / 기대) |
|---|---|---|
| evaluate::run::tests::saves_magnitude_and_display_rule_needs_both_scores | src/server/evaluate/run_tests.rs:594 | (3,2,0) / (3,1,0) |
| keywords::performance::tests::gated_links_count_as_matched_and_pass_follows_the_display_rule | src/server/keywords/performance_tests.rs:100 | passed 2 / 1 |
| schedule::plan::tests::regenerates_stocks_whose_keywords_were_made_before_this_cycle | src/server/schedule/plan_tests.rs:213 | every Weekly, ids [4] / Daily, [1,4] |
| schedule::plan::tests::skips_generating_stocks_and_auto_topic_lens_items | 같은 파일 | 기본 주기 Weekly |
| schedule::scheduler::tests::a_failing_plan_read_does_not_stop_the_other_jobs | src/server/schedule/scheduler_tests.rs:270 | [] / [1] |
| schedule::scheduler::tests::a_refused_collection_start_does_not_stop_keywords_and_listing_check | 같은 파일 | 같은 계열 |
| schedule::scheduler::tests::at_start_catches_up_missed_collection_keyword_regeneration_and_listing_check | 같은 파일 | 같은 계열 |
| schedule::scheduler::tests::keyword_regeneration_runs_once_per_cycle | 같은 파일:214 | [] / [1] |
| timeline::read::tests::read_timeline_filters_by_relevance_orders_newest_and_skips_deleted | src/server/timeline/read_tests.rs:272 | 5.0 / 6.0 |
| timeline::read::tests::read_timeline_needs_both_scores_and_old_evaluations_use_relevance_only | 같은 파일 | 같은 계열 |
| web::timeline::tests::nothing_over_the_bar_says_so | src/web/timeline_tests.rs | 영향도 6점 문구 |
| web::timeline::tests::shows_cards_by_day_with_filters_and_threshold_text | src/web/timeline_tests.rs:134 | "관련도 6점·영향도 6점 이상인 기사를…" |

## 시도한 것
- 범위 안 시험은 고쳤다. `settings_form_tests.rs`의 `valid()`(깊이 "3"→"1", 주기 "weekly"→"daily", 영향도 "6"→"5")와 깊이 한계 메시지(1~4→1~2, 경계 "5"→"3"). `worker/server_tests.rs`의 설정 입력도 같은 이유로 바꿨다(깊이 3이 이제 400). 바뀐 항목 목록 기대값은 그대로다.
- 범위 밖 12개는 건드리지 않았고, 기본값도 되돌리지 않았다(브리프의 미리 정한 판단).
- 필요한 결정(내가 하지 않음): 위 12개 시험의 기대값·시드 값을 새 기본값에 맞춰 고칠 범위를 허용할지, 아니면 기본값 적용을 다른 방식으로 할지.

## 끝낸 것
- 체크포인트 1(트리 화면 실적): 키워드 줄에 `기사 N · 통과 M`(`keyword-stats`), 최근 28일, 0도 보임, 점수 자리 제거. `stocks_tests` 통과.
- 체크포인트 2(별칭): 종목 이름 줄 아래 "별칭" 칩(`stock-alias`), 주인에게만 `alias-remove` 버튼. web `POST /stocks/{id}/aliases/remove`(필드 `alias`) → `worker_client.remove_alias` → worker `POST /stocks/{id}/aliases/remove`(JSON `{"alias":…}`)가 `remove_alias`를 부름. 없는 종목 404 `not_found`, 없는 별칭 404 `alias_not_found`, 잘못된 본문 400, 토큰 없음 401. web·worker·worker_client 시험 통과.
- 체크포인트 3(설정): 깊이 1~2만 받음(3이면 오류), 기본값 깊이 2·주기 weekly·영향도 5·토픽 기준 6·5, 칸 이름을 "토픽을 종목에 붙일 관련도/영향도(0~10)"로 변경, 저장된 깊이 3·4는 화면에 2로 보임. `settings_form_tests`·`settings_tests` 통과.

## 하지 못한 것
체크포인트 4(`e2e/keywords.spec.ts`·`e2e/settings.spec.ts` 수정, README, `/check` 전체, 캡처). `/check`는 위 12개 실패 때문에 통과하지 못한다.

## 브리프에 없던 결정
- 별칭 지우기 버튼은 손님(`!access.is_member()`)에게는 그리지 않는다(`StockView.alias_editable`). 손님은 칩만 본다.
- 이미 없는 종목·별칭(worker 404)은 목록을 다시 그려 끝난 것으로 처리하고, 그 밖의 실패는 칩 옆 글로 보인다.
- worker 본문 `alias`는 다듬지 않고 1~100자만 받는다.
- 키워드 요약 줄 "깊이 N"은 기존 계산(키워드 level 최댓값)을 그대로 뒀다.
- 정기 점검 주기 칸 옵션 글에서 "(매일 06:00)"·"(월요일 06:00)"를 빼고 "매일 06:00에 확인하고, 이 주기가 지난 종목만 정기 점검합니다." 설명을 더했다. 깊이 칸 설명은 "다음 정기 점검 때 반영됩니다."로, 토픽 기준 칸 설명은 토픽 전용 문구로 고쳤다.
- `web/stocks.rs`의 `load`가 `now`를 받도록 바꿨고(최근 28일 기준), `stocks_tests.rs`의 `keyword_tree`·`tree_node` 호출에 빈 실적 표를 넘기게 고쳤다.
- 범위 밖 파일은 고치지 않았다. `git status`상 새로 생긴 범위 밖 변경은 없다(`work/metrics.csv`는 시작 전부터 있던 미추적 파일).
