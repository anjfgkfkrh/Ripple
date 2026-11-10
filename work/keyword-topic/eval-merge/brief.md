# eval-merge: 시장·토픽 기사를 기사당 Jev 한 요청으로 평가
모델: Sonnet / effort: medium | 선행: anchor-match 통과

## 목표
시장 항목(한국·미국 증시)과 주제 항목(사용자 주제·자동 토픽의 증시별 항목)에 걸린 같은 기사의 연결들을 Jev 한 요청으로 묻는다. 토픽마다 관련도, 시장 항목마다 관련도, 증시마다 영향도·호재/악재를 받는다. 결과는 연결마다 평가 한 줄로 저장하고, 사용량은 연결 수로 나눠 남긴다. 종목 연결은 지금처럼 하나씩 묻는다.

## 완료 조건
- `questions_tests` 새 시험 통과: 토픽 2개(한국·미국 증시 항목) + 한국 시장 항목이 걸린 기사의 state·questions가 아래 모양·문구와 글자까지 같음, 증시가 하나뿐이면 그 증시 질문만
- `typesafe_tests` 새 시험 통과: 합친 응답 해석(키마다 점수 ×2·확신도, 선택 확률), 키가 빠지면 형식 오류
- `evaluate/run_tests` 새 시험 통과:
  - 같은 기사의 시장·토픽 연결 셋이 호출 1번
  - 토픽 항목(T, 증시 M)은 T 관련도·M 영향도·M 호재/악재, 시장 항목 M은 M 관련도·영향도·호재/악재로 저장
  - 사용량이 연결마다 한 줄(토큰·비용을 나눔, 합이 원래 값)
  - 회차 한도는 호출 1번으로 셈
  - 같은 소식 복사는 연결마다 먼저 보고 남은 것만 묻음
  - 실패하면 그 연결들이 다음 회차에 남음
  - 종목 연결은 지금과 같음
- `/check` 전체 통과(E2E 포함)

## 수정 가능 범위
- `rust/ripple/src/server/ai/decider/`
- `rust/ripple/src/server/ai/decider.rs`
- `rust/ripple/src/server/evaluate/`
- `rust/ripple/src/server/db/schema.rs`(`EvaluationSummary`에 칸이 필요할 때만)
- `rust/ripple/src/test_support.rs`(시험용 가짜 `Decider`에 새 함수)
- `rust/ripple/src/server/keywords/generate.rs`(`NoDecider`에 새 함수만)
- `rust/ripple/src/server/budget/usage_tests.rs`(시험용 가짜 `Decider`에 새 함수만)
- `e2e/mock-services.mjs`

## 볼 위치
- `scripts/analysis/keyword-topic/jev_make.py`의 (c) 부분(기사당 한 요청의 state·questions), `merged_request`, `TOPIC_REL`·`MARKET_MAG`·`IMPACT`·`MARKET_NAME`
- `server/ai/decider/questions.rs`(`article_relevance_levels`, `MARKET_MAGNITUDE_LEVELS`, `score`, `target_state`), `types.rs`(`Decider`), `typesafe.rs`(`decide`, `score_answer`)
- `server/evaluate/run.rs`의 `pending_evaluations`, `find_same_news_evaluation`, `evaluate_pending`
- `server/budget/usage.rs`의 `record_usage`
- `e2e/mock-services.mjs`의 `/typesafe/v1/systemone` 처리(질문 모양을 엄격히 검사함)
- `docs/DECISIONS.md` 2장 "키워드·토픽 고도화 유료 확인"의 "기사당 한 요청" 줄, 1장 "유료 확인 뒤 결정" ①, "17단계 `keyword-topic` 계획 답" ⑨, "17단계 계획 때 정한 세부" ⑦

## 미리 정한 판단
- 모양(실측한 (c)와 같게, `link`는 넣지 않음):
  - state: `{"topics": {"t1": {"name", "summary"?}, …}, "markets": {"kr": {"name": "Korean stock market (KOSPI, KOSDAQ)"}, "us": {"name": "US stock market (S&P 500, Nasdaq)"}}, "article": {"title", "summary"?}}`
  - `topics`에는 그 기사에 걸린 서로 다른 토픽만 걸린 순서대로 `t1`부터 넣는다. 토픽이 없으면 `topics`를 뺀다.
  - `markets`에는 걸린 항목의 증시만 넣는다.
  - 질문 `relevance_t{n}`: `score("How directly is the `article` about the topic in `topics.t{n}`?", 주제 기사 관련도 6단계)`
  - 질문 `relevance_{m}`(그 증시의 시장 항목이 걸렸을 때만): `score("How directly is the `article` about the overall stock market in `markets.{m}`?", 시장 기사 관련도 6단계)`
  - 질문 `magnitude_{m}`: `score("How much could the news in the `article` move the overall stock market in `markets.{m}`?", MARKET_MAGNITUDE_LEVELS)`
  - 질문 `impact_{m}`: `choice("What effect does the `article` have on the overall stock market in `markets.{m}`?", 기존 positive·negative·neutral 기준)`
  - 질문 순서: 관련도(토픽 → 시장), 영향도, 호재/악재.
  이유: 유료 실측 형식이다(71% 절감, 통과 일치 86·90/95). 시장 항목 관련도는 실측에 없던 질문이라 stage-check가 따로 비교한다. 버린 대안: 한국·미국 쌍만 합침
- `Decider`에 합친 평가 함수를 하나 더한다(이름 자유). 입력은 기사·토픽 목록·시장 항목 증시 목록·증시 목록, 출력은 키별 점수(0~10)·확신도, 증시별 호재/악재·확신도·확률, 모델, 토큰. 기존 `decide`·`rate`는 그대로 둔다.
- 묶기: 지금 순서대로 대기 연결을 훑다가 시장·주제 항목 연결을 만나면, 그 기사의 시장·주제 항목 대기 연결을 모두 모은다. 연결마다 같은 소식 복사를 먼저 보고, 남은 것만 한 요청으로 보낸다. 회차 한도·예산 검사는 요청 1번으로 센다.
- 저장: 연결마다 `evaluations` 한 줄. `copied_from`은 NULL, `model`은 응답 모델. 사용량은 연결마다 `record_usage` 한 줄(`purpose = "article_evaluation"`, 그 연결의 `stock_id`)이다. 입력·출력 토큰은 연결 수로 나누고 나머지는 첫 줄에 더한다. 비용은 그 줄의 입력 토큰 × 단가다.
  이유: 사용자 답 9A(항목별 비용 유지). DB 구조는 바꾸지 않는다.
- 요약 `evaluated`는 저장한 평가 줄 수, `failed`는 실패한 요청 수로 센다.

## 체크포인트 (각각 검사 통과 후 커밋)
1. 질문·state 만들기 → `questions_tests` 새 시험 통과
2. `Decider` 새 함수와 TypeSafe 해석 → `typesafe_tests` 새 시험 통과
3. 평가 묶기·저장·사용량 → `evaluate/run_tests` 새 시험 통과
4. E2E 가짜 서버 → `/check` 전체 통과

## 제약
- `questions_fixture.json`과 기존 `questions_tests`·`typesafe_tests`·`evaluate/run_tests`의 기대값은 바꾸지 않는다. 기존 시험 가운데 시장·주제 연결을 하나씩 묻는다고 기대하는 것만 새 규칙으로 고치고, 고친 시험 이름을 인계에 적는다.
- 실제 API를 부르지 않는다. `docs/`는 고치지 않는다(인계에 적음).

## 알려진 함정
- 실측에서 합친 질문의 영향도가 약 0.3 낮게 나왔다(통과가 조금 줄 수 있음). 고치려 하지 말고 그대로 둔다.
- 같은 기사가 같은 토픽의 한국·미국 항목에 모두 걸리면 토픽은 `t1` 하나, 증시는 둘이다.
- 리뷰(2026-11-10, anchor-match): 평가 대기를 고르는 `evaluate/run.rs` `pending_evaluations`·`count_pending_evaluations`와 화면용 `evaluate/read.rs` `count_pending_evaluations`에 `article_matches.gated = 0` 조건이 있다. 기사당 한 요청으로 묶도록 질의를 바꿀 때 이 조건을 잃지 않는다(시장·토픽 항목은 걸러지지 않지만 같은 기사의 종목 연결이 걸러질 수 있음).

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고) + DB 구조를 바꿔야 할 것 같을 때
