# stock-subject: 종목 평가에 예/아니오 질문 `bystander`를 더한다
모델: Sonnet / effort: medium | 선행: measure 통과, schema 통과

## 목표
종목 기사 평가(`decide`) 요청에 예/아니오 질문 `bystander`(API `type` 이름 `"noul"`)를 더한다. 이 질문은 "다른 대상이 받는 영향을 보도하고 이 회사는 원인이나 지나가는 언급으로만 나오는가"를 묻는다. 받은 확률은 `evaluations.bystander`에 저장하고, 같은 소식 복사 때도 옮긴다. 화면 숨김은 `display-rule`이 한다.

## 완료 조건
- `questions_tests` 새 시험 통과: 종목 질문이 relevance·magnitude·impact·bystander 순서이고, `bystander`가 아래 글자 그대로. 시장·주제 기사 질문(`build_article_questions(Market|Topic)`, 합친 질문)에는 없음
- `questions_fixture.json`의 `articleQuestions.stock`에 `bystander`를 더해 고정본 비교 시험이 통과
- `typesafe_tests` 새 시험 통과: 응답의 예/아니오 답 `answers.bystander.noul`에서 확률을 읽어 `Decision`에 담음. 답이 없거나 확률이 숫자가 아니면 형식 오류(`Upstream`)
- `evaluate/run_tests` 새 시험 통과: 종목 평가 줄에 `bystander` 저장, 같은 소식 복사에 앞선 평가의 `bystander`(NULL이면 NULL)가 옮겨짐, 시장·주제 평가 줄은 NULL
- E2E 가짜 TypeSafe가 종목 요청의 `bystander`에 답하고(기본 낮은 확률), `/check` 전체 통과(E2E 포함)

## 수정 가능 범위
- `rust/ripple/src/server/ai/decider/`
- `rust/ripple/src/server/evaluate/run.rs`
- `rust/ripple/src/server/evaluate/run_tests.rs`
- `rust/ripple/src/test_support.rs`(가짜 `Decider`의 답에 새 값)
- `rust/ripple/src/server/budget/usage_tests.rs`(가짜 `Decider`가 `Decision`을 만들 때만)
- `rust/ripple/src/server/keywords/generate.rs`(`NoDecider`가 `Decision`을 만들 때만)
- `e2e/mock-services.mjs`

## 볼 위치
- `questions.rs`의 `build_article_questions`, `score`, `impact_choice`
- `questions_tests.rs`의 `compare_with_fixture`
- `typesafe.rs`의 `decide`와 응답 해석(`score_answer`, `impact`의 `choice`)
- `types.rs`의 `Decision`
- `evaluate/run.rs`의 `store_evaluation`, `store_copy`, `find_same_news_evaluation`, `Evaluation`
- `e2e/mock-services.mjs`의 `/typesafe/v1/systemone`
- `work/article-classify/measure/handoff.md`(예/아니오 응답 원문과 측정 결과)

## 미리 정한 판단
- 질문 키 `bystander`, 모양 `{"type": "noul", "instructions": "Does the `article` mainly report how something else (a stock index, another company, or an institution) is affected, with the company in `stock` appearing only as a cause or in passing?"}`. 종목 대상일 때만 넣고, 순서는 impact 뒤.
  이유: 계획 답 ③. `measure`가 이 글자로 쟀다. 상위가 측정 뒤 글자를 바꾸면 이 줄을 고쳐 둔다.
- 응답 해석은 `measure` 인계의 응답 원문 모양을 따른다: `answers.bystander = {"type": "noul", "noul": <예일 확률 0~1>}`. `noul` 숫자만 쓴다.
  이유: 계획 답 ②(확률 그대로 저장, 기준은 상수). 계획 세부 ⑦(measure 첫 호출로 확인, `"boolean"`은 HTTP 400)
- `Decision`에 `bystander: Option<f64>`를 더한다. 종목 대상이면 Some, 그 밖은 None. 합친 평가(`MergedDecision`)와 `rate`는 바꾸지 않는다.
- `questions.ts`는 14단계에 지워졌다. 고정본 JSON만 손으로 고치고, 모듈 맨 위 설명의 "`questions.ts`와 글자까지 같아야 한다"는 "고정본 `questions_fixture.json`과 글자까지 같아야 한다"로 고친다.
  이유: 계획 세부 ⑤
- 같은 소식 복사는 앞선 평가의 `bystander`를 그대로 옮긴다(앞선 평가가 이 질문 전 것이면 NULL).

## 체크포인트 (각각 검사 통과 후 커밋)
1. 질문과 고정본 → `questions_tests` 통과
2. 응답 해석과 `Decision` → `typesafe_tests` 새 시험 통과
3. 저장·복사와 가짜 서버 → `evaluate/run_tests` 새 시험 통과, `/check` 전체 통과

## 제약
- 표시 규칙(`display_rule`, `meets_display_rule`)은 이 작업에서 바꾸지 않는다.

## 알려진 함정
- E2E 가짜 TypeSafe는 질문 모양을 엄격히 검사한다(17단계 eval-merge). 종목 요청에 새 키가 있다는 것을 그 검사에 맞춰 넣는다.
- 리뷰(2026-11-14, measure): 예/아니오 질문 `type`은 `"boolean"`이 아니라 `"noul"`이다. SPEC 291줄의 "boolean"은 유형 이름일 뿐 요청 글자가 아니다.

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고) + `measure` 인계의 응답 모양이 이 브리프와 맞지 않음
