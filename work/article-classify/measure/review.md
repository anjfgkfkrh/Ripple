# article-classify/measure 리뷰
결과: 통과
범위 밖 무기록: 0

## 범위 확인
- 검사 명령 재실행: 인계 숫자와 일치. `.check-restore/ripple.db` 사본으로 `measure_free.py`를 다시 돌린 출력이 `free.txt`와 같고, `measure_read.py work/.../measure/data`의 출력이 `consistency.txt`와 같다(응답 300개). 유료 호출은 다시 하지 않음. `ripple-check` 컨테이너 없음, `data/`는 `.gitignore`로 커밋 밖, 리뷰용 사본은 지움.
- 범위 밖 판단: 없음(바뀐 파일은 `scripts/analysis/article-classify/` 5개와 작업 폴더뿐. 미추적 `work/metrics.csv`·`work/article-classify/metrics.csv`는 작업 전부터 있음)

## Opus 확인
- 브리프에 없던 결정이 인계에 빠짐없이 있는가: 예. `type` `"noul"`(브리프는 `"boolean"`, 400 뒤 SDK 타입으로 확인), 형식 표본 고르기 규칙과 한계(`[자막뉴스]`만, 뉴스 모음 2건은 단일 주제, 기자수첩 기준 미달), c의 시장 관련도 최댓값·이름 연결 미한정, a의 similar_title 이식, 점수 ×2.
- 테스트 기대값을 바꾸거나 빼서 통과시키지 않았는가: 예(시험 코드 변경 없음, 측정 작업).
- 제약과 프로젝트 규칙을 지켰는가: 예. `-p ripple-check`만, 키는 컨테이너 환경 변수(`run.sh`가 출력하지 않음), `rust/`·`docs/` 변경 없음, `python3 -I`, 비용 $0.0101 ≤ $0.04. 브리프 문구 "`type: boolean`"을 바꾼 것은 응답 모양 확인이 이 작업의 목적이고 400은 과금 없이 끝나 정당하다.
- 인계가 실제 변경과 맞는가: 예. 스크립트의 질문 글자가 브리프와 같고(`NATURE`·`BYSTANDER`), 종목 질문은 고정본 `articleQuestions.stock`을 그대로 읽는다. 첫 응답 원문이 `first-response.json`과 같다.
- 참고: 비용은 입력 토큰만 셌다(브리프 정의대로). 출력 토큰 28,350의 요금은 모름.

## 반영한 것 (통과일 때)
- 영구 문서: DECISIONS 1장 "18단계 계획 때 정한 세부" ⑦에 확인 결과·근거, 새 줄 "18단계 measure 구현 세부"(①~⑤, 근거: article-classify/measure), 2장 "기사 분류 측정"(a~d, 응답 모양, 일관성, `bystander`, 성격 태그, 점수 비교)과 "다시 잴 때의 방법" 한 줄. SPEC 291줄 boolean에 API 이름 `"noul"`. STATE 다시 씀(사람 확인 항목: 지난 평가 규칙 79건 중 ④ 23건, 7일 성격 비용, `bystander` 묶음별, `[자막뉴스]` `media` 미검출).
- 다음 브리프 확인: `schema`는 그대로 맞음. `stock-subject` 브리프의 질문 모양·응답 해석을 `"noul"`/`answers.bystander.noul`로 고치고 함정 한 줄. `article-nature`(대기)에 `"noul"` 함정 한 줄. 질문 문구·기준 0.5는 바꾸지 않음(`[자막뉴스]` media 문구를 바꿀지는 사람 판단으로 STATE에 올림).
