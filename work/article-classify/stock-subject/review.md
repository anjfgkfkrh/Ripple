# article-classify/stock-subject 리뷰
결과: 통과
범위 밖 무기록: 0

## 범위 확인
- 검사 명령 재실행: 인계 숫자와 일치(`/check` R1~R4 통과, cargo test 746 통과 0 실패, pnpm test 7 통과, E2E 49 통과 0 실패 5 건너뜀)
- 범위 밖 판단:
  - 없음(바뀐 코드 파일은 모두 수정 가능 범위 안. 미추적 `work/metrics.csv`·`work/article-classify/metrics.csv`는 작업 전부터 있던 측정 기록)

## Opus 확인
- 브리프에 없던 결정이 인계에 빠짐없이 있는가: 예. `store_evaluation` 인자, `BYSTANDER_INSTRUCTIONS` 상수, 가짜 기본값 0.1, E2E 422 조건이 인계에 있음
- 테스트 기대값을 바꾸거나 빼서 통과시키지 않았는가: 예. 기존 시험은 `Decision` 기대값에 새 칸 `bystander: Some(0.3)`과 가짜 답에 새 키를 더했을 뿐이고, 고정본 JSON은 브리프 지시대로 더함
- 제약과 프로젝트 규칙을 지켰는가: 예. 표시 규칙(`display_rule`, `meets_display_rule`)과 `MergedDecision`·`rate`는 그대로
- 인계가 실제 변경과 맞는가: 예. diff와 일치. 시장·주제 단독 평가도 `decide`가 `None`을 돌려 NULL로 저장됨

## 반영한 것 (통과일 때)
- 영구 문서: DECISIONS 1장에 "18단계 stock-subject 구현 세부" 줄 추가(근거 붙임). STATE 다시 씀(stock-subject 통과, 다음 display-rule, measure 답 대기 항목은 사람이 진행시켰으므로 남은 질문 `[자막뉴스]`·`roundup`만 남김). SPEC 291줄은 이미 `"noul"`을 적고 있어 고치지 않음
- 다음 브리프 확인: `display-rule` 브리프는 이번 인계와 맞음. "알려진 함정"에 가짜 평가의 `bystander` 기본값(0.1 vs SQL 직접 입력 NULL) 차이를 한 줄 더함
