# article-nature: 보이는 기사의 성격을 묻고 사진영상·뉴스 모음을 숨긴다
모델: Sonnet / effort: medium | 선행: display-rule 통과

## 목표
평가가 끝난 뒤 같은 회차에서, 화면 기준을 넘었는데 아직 성격이 없는 기사마다 Jev 별도 요청 1번으로 성격 6개(예/아니오)를 묻는다. 결과는 `articles.nature_*`에 저장한다. 같은 소식 기사에 앞선 답이 있으면 묻지 않고 복사한다. `media`나 `roundup`이 기준 이상인 기사는 숨김 규칙에 더해 어디에도 보이지 않게 한다.

## 완료 조건
- `questions_tests` 새 시험 통과: 성격 state·questions가 아래 글자 그대로
- `typesafe_tests` 새 시험 통과: 6개 확률 해석, 하나라도 빠지면 형식 오류
- `evaluate` 시험 새 시험 통과:
  - 기준을 넘은 평가가 있고 `nature_at`이 NULL인 7일 안 기사만 물음, 기준 미만·숨김(`bystander`)뿐인 기사·7일 밖 기사는 묻지 않음
  - 같은 소식(24시간 안 비슷한 제목) 기사에 `nature_at`이 있으면 복사하고 호출하지 않음
  - 사용량 한 줄(`purpose = 'article_nature'`, `stock_id` NULL, 비용 = 입력 토큰 × 단가)
  - 회차 한도는 평가 호출과 합쳐 셈, 한도·예산에 닿으면 남은 기사는 다음 회차로
  - 실패하면 `nature_at` NULL로 남아 다음 회차에 다시 물음
  - 요약에 물은 수·복사한 수·실패 수
- 숨김 시험 통과: `nature_media` 0.8인 기사는 타임라인·토픽 수·키워드 통과·통계 낱말에서 빠지고, 성격이 NULL인 기사는 보임
- E2E 가짜 TypeSafe가 성격 요청에 답하고 `/check` 전체 통과(E2E 포함)

## 수정 가능 범위
- `rust/ripple/src/server/ai/decider/`
- `rust/ripple/src/server/evaluate/`
- `rust/ripple/src/server/db/schema.rs`(`EvaluationSummary`에 칸)
- `rust/ripple/src/server/timeline/read.rs`
- `rust/ripple/src/server/timeline/read_tests.rs`
- `rust/ripple/src/server/topics/store.rs`
- `rust/ripple/src/server/topics/topics_tests.rs`
- `rust/ripple/src/server/keywords/performance.rs`
- `rust/ripple/src/server/keywords/performance_tests.rs`
- `rust/ripple/src/server/keywords/stat_words.rs`
- `rust/ripple/src/server/keywords/stat_words_tests.rs`
- `rust/ripple/src/test_support.rs`(가짜 `Decider`에 새 함수)
- `rust/ripple/src/server/budget/usage_tests.rs`(가짜 `Decider`에 새 함수만)
- `rust/ripple/src/server/keywords/generate.rs`(`NoDecider`에 새 함수만)
- `e2e/mock-services.mjs`

## 볼 위치
- `work/article-classify/measure/handoff.md`(boolean 응답 모양, 성격 질문 측정 결과)
- `work/article-classify/stock-subject/handoff.md`(boolean 해석을 어디에 두었는지)
- `server/db/schema.rs`의 `NATURE_THRESHOLD`, 태그·숨김 고르기 함수(schema 작업)
- `server/evaluate/`의 숨김 SQL 조각(display-rule 작업), `evaluate_pending`의 회차 한도·연속 실패·예산 처리
- `server/budget/usage.rs`의 월 한도 검사와 `record_usage`
- `server/similar_title.rs`의 `is_same_news`, `SAME_NEWS_WINDOW_MS`
- `server/collect/run.rs`의 `evaluate_pending` 호출과 요약 저장

## 미리 정한 판단
- `Decider`에 `classify_nature(article: &DecisionArticle, model) -> NatureDecision`(6개 확률, 모델, 토큰)을 더한다. state `{"article": {"title", "summary"?}}`. questions는 이 순서와 글자 그대로, 모두 `{"type": "boolean", "instructions": …}`:
  - `fact`: "Does the `article` report facts: events, announcements, results, or market moves that have already happened?"
  - `analysis`: "Does the `article` interpret the causes, meaning, or background of the news, beyond restating the facts?"
  - `forecast`: "Does the `article` present a prediction or outlook, such as a target price, an earnings estimate, or an expected market direction?"
  - `opinion`: "Is the `article` an editorial, column, or opinion piece arguing the writer's position?"
  - `media`: "Is the `article` mainly a photo or video, with only a caption or short description?"
  - `roundup`: "Is the `article` a roundup that lists several unrelated news items?"
  이유: 계획 답 ①③⑧. `measure`가 이 글자로 쟀다. 상위가 측정 뒤 글자를 바꾸면 이 줄을 고쳐 둔다.
- 묻는 곳: `evaluate_pending`이 평가를 마친 뒤 같은 함수 안(또는 거기서 부르는 함수)에서 남은 회차 한도만큼 묻는다. 대상은 `published_at >= now - EVALUATION_WINDOW_MS`이고 `nature_at IS NULL`이다. 또 지우지 않은 항목의 평가 중 관련도·영향도 기준과 숨김 SQL 조각(성격 부분 제외)을 넘는 것이 하나라도 있어야 한다. 순서는 `published_at` 최근순이다.
  이유: 계획 세부 ③, 계획 답 ⑨(7일 창 안 성격 없는 보이는 기사를 물어 지난 7일도 저절로 한 번 묻는다)
- 묻기 전에 같은 소식을 찾는다. 발행 ±`SAME_NEWS_WINDOW_MS` 안에서 `nature_at`이 있는 기사 가운데 `is_same_news`인 첫 기사의 6개 확률을 복사하고 `nature_at`을 쓴다. 항목은 따지지 않는다.
  이유: 계획 답 ①(같은 소식 복사)
- 사용량: 요청마다 `record_usage` 한 줄, `purpose = "article_nature"`, `stock_id` NULL, 비용 = 입력 토큰 × `jev.usdPerMillionInputTokens`. 예산 검사는 평가 호출과 같은 방식으로 호출 전에 한다.
  이유: 계획 세부 ③(항목별 비용에서 공용)
- 숨김: display-rule의 SQL 조각에 `COALESCE(articles.nature_media, 0) < ? AND COALESCE(articles.nature_roundup, 0) < ?`(기준 `NATURE_THRESHOLD`)를 더한다. 이 조각을 쓰는 조회는 모두 `articles`를 붙인다. 묻기 대상 고르기에는 이 성격 부분을 넣지 않는다.
  이유: 계획 답 ⑦⑧. NULL은 숨기지 않는다(4A).
- 요약 `EvaluationSummary`에 `nature_asked`·`nature_copied`·`nature_failed`(serde 기본 0)를 더한다. 회차 결과 로그 한 줄에도 붙인다.
- E2E 가짜 TypeSafe 성격 답: 제목에 "[포토]"가 있으면 `media` 0.9, "전망"이 있으면 `forecast` 0.9·`fact` 0.2, 그 밖은 `fact` 0.9. 적지 않은 것은 모두 0.1.
  이유: `nature-tabs` E2E가 이 규칙으로 태그와 탭을 확인한다.

## 체크포인트 (각각 검사 통과 후 커밋)
1. 질문·state·`Decider` 새 함수·해석 → `questions_tests`·`typesafe_tests` 새 시험 통과
2. 묻기·복사·사용량·요약 → `evaluate` 새 시험 통과
3. 숨김 조각과 가짜 서버 → 숨김 시험 통과, `/check` 전체 통과

## 제약
- 평가(`decide`, `decide_merged`)의 질문·저장은 바꾸지 않는다.
- 7일보다 오래된 기사는 묻지 않는다.

## 알려진 함정
- 리뷰(2026-11-14, measure): 예/아니오 질문 `type`은 브리프의 `"boolean"`이 아니라 `"noul"`이고 답은 `answers.<이름>.noul`(0~1)이다(`"boolean"`은 HTTP 400). measure도 `"noul"`로 쟀다.
- 리뷰(2026-11-14, schema): `server/db/schema.rs`에 이미 자유 함수 `classify_nature(&NatureProbs) -> (Vec<NatureTag>, bool)`와 구조체 `NatureProbs`(f64 6칸)가 있다. `Decider` 메서드를 같은 이름으로 두면 헷갈리니 6확률은 `NatureProbs`로 담고, 숨김 기준은 `NATURE_THRESHOLD`를 쓴다.

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고)
