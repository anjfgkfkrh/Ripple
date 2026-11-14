# display-rule 리뷰
결과: 통과
범위 밖 무기록: 0

## 범위 확인
- 검사 명령 재실행: 인계 숫자와 일치(cargo test 752 통과·0 실패, E2E 49 통과·0 실패·5 건너뜀(모바일), fmt·clippy·build·pnpm 형식·린트·타입·단위 통과)
- 범위 밖 판단:
  - 없음(바뀐 파일 9개 모두 수정 가능 범위 안, 그 밖은 PLAN.md·작업 폴더 기록)

## Opus 확인
- 브리프에 없던 결정이 인계에 빠짐없이 있는가: 예. `?3` 바꿔 넣기, `keyword_performance`의 `INNER JOIN stocks`, 토픽 카드 두 조회의 `INNER JOIN stocks`(한 일에 적음), 속도 미측정을 적었다. `article_matches.stock_id`는 NOT NULL이라 조인으로 행이 줄지 않는다.
- 테스트 기대값을 바꾸거나 빼서 통과시키지 않았는가: 예. 기존 시험 diff 없음, 새 시험만 더함.
- 제약과 프로젝트 규칙을 지켰는가: 예. 저장된 평가를 고치지 않음, `meets_display_rule` 서명 그대로, 요약 `relevant` 그대로. 조각은 브리프 모양과 글자까지 같고 `COALESCE`로 NULL 행이 빠지지 않는다(`LEFT JOIN evaluations`로 평가가 없는 행도 NOT(...)가 참이 되어 셈이 깨지지 않음). `topic_article_counts`는 `shown`이 SELECT에 있어 기준 값을 앞에 넣는 순서가 맞다.
- 인계가 실제 변경과 맞는가: 예. 다만 완료 조건의 "토픽 카드" 새 시험이 없었다(`read_topic_cards`는 붙은 종목 조회에 조각이 걸리는데 시험 없음). 리뷰가 `read_topic_cards_apply_the_hidden_rule_to_attached_stocks_only`를 더해 별도 커밋(1103742)했다: 토픽 항목 평가는 `bystander` 0.9여도 세고, 붙은 종목의 0.7 평가는 영향에서 빠진다. 통과.

## 반영한 것 (통과일 때)
- 영구 문서: DECISIONS.md 1장에 "18단계 display-rule 구현 세부" 줄(근거: article-classify/display-rule). STATE.md 다시 씀(display-rule 통과, 다음 article-nature, 속도 미측정은 stage-check에서). SPEC은 18단계 줄에 이미 있어 고치지 않음.
- 다음 브리프 확인: `article-nature` 브리프의 성격 질문 `type`을 `"boolean"` → `"noul"`로 고침(measure 확인). 알려진 함정에 숨김 조각 위치·쓰는 조회·`replace('?', "?3")`가 성격 `?`를 한 값으로 묶는 문제·`articles`가 없는 조회 3곳을 적음.
