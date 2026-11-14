# display-rule: 숨김 규칙 하나를 모든 통과 셈에 적용
모델: Sonnet / effort: medium | 선행: stock-subject 통과

## 목표
화면 표시 기준(관련도·영향도)에 숨김 규칙을 더해, 타임라인·토픽 기사 수·키워드 통과 수·통계 낱말이 모두 같은 규칙을 쓰게 한다. 숨김 규칙은 종목 항목 평가에만 적용한다. 하나는 새 질문 `bystander`의 확률이 기준 이상이면 숨기는 것이다. 다른 하나는 `bystander`가 없는 평가에서, 같은 기사의 시장 항목 평가 관련도가 그 종목 관련도보다 크면 숨기는 것이다.

## 완료 조건
- `timeline/read_tests` 새 시험 통과:
  - 종목 평가 `bystander` 0.7 → 그 종목은 카드에서 빠지고, 같은 기사의 시장 항목이 기준 이상이면 카드는 시장으로 남음
  - `bystander` 0.3 → 보임
  - `bystander` NULL이고 같은 기사 시장 항목 관련도 9 > 종목 7 → 종목 빠짐, 시장 7 ≤ 종목 7 → 보임, 시장 평가가 없으면 보임
  - 종목 필터(`?stock=`)에서도 같은 규칙
  - 시장·주제 항목은 이 규칙을 받지 않음
- 토픽 카드·토픽 기사 수(`topic_article_counts`), `keyword_performance` 통과, `stat_words` 대상에 같은 규칙이 적용되는 새 시험 통과
- 기존 시험 기대값은 바꾸지 않음. 같은 기사에 종목·시장 평가가 함께 있어 새 규칙으로 깨지는 기존 시험은 시험 준비를 고쳐(종목 관련도를 시장 이상으로, 또는 `bystander` 0) 원래 시나리오를 지키고 인계에 목록을 적는다
- `/check` 전체 통과(E2E 포함)

## 수정 가능 범위
- `rust/ripple/src/server/evaluate/`
- `rust/ripple/src/server/timeline/read.rs`
- `rust/ripple/src/server/timeline/read_tests.rs`
- `rust/ripple/src/server/topics/store.rs`
- `rust/ripple/src/server/topics/topics_tests.rs`
- `rust/ripple/src/server/keywords/performance.rs`
- `rust/ripple/src/server/keywords/performance_tests.rs`
- `rust/ripple/src/server/keywords/stat_words.rs`
- `rust/ripple/src/server/keywords/stat_words_tests.rs`

## 볼 위치
- `timeline/read.rs`의 `display_rule`과 그것을 쓰는 세 곳(`read_timeline`, 토픽 카드, 그 밖)
- `evaluate/run.rs`의 `meets_display_rule`과 `store_copy`·`store_evaluation`의 반환값(요약 `relevant` 셈)
- `topics/store.rs`의 `topic_article_counts`(통과 SQL 글자가 따로 있음), `topics/run.rs`의 호출
- `keywords/performance.rs`의 `keyword_performance`, `keywords/stat_words.rs`의 `stat_words`
- `docs/DECISIONS.md` 1장 "18단계 `article-classify` 계획 답" ⑤⑦

## 미리 정한 판단
- 숨김 조건은 SQL 조각 하나로 `server/evaluate/`에 둔다. 조회에 `evaluations`·`stocks` 별칭이 있어야 하고, 기준은 `?` 자리에 `BYSTANDER_THRESHOLD`를 넣는다. 모양:
  `NOT (stocks.kind = 'stock' AND (COALESCE(evaluations.bystander, 0) >= ? OR (evaluations.bystander IS NULL AND EXISTS (SELECT 1 FROM evaluations AS m INNER JOIN stocks AS ms ON ms.id = m.stock_id WHERE m.article_id = evaluations.article_id AND ms.kind = 'market' AND ms.deleted_at IS NULL AND m.relevance > evaluations.relevance))))`
  NULL 비교가 NULL이 되어 행이 빠지지 않게 `COALESCE`를 쓴다. `bystander` NULL·시장 평가 없음인 종목이 보이는 시험을 꼭 둔다.
  `display_rule`(타임라인)과 `topic_article_counts`의 통과 조건은 관련도·영향도 조건 뒤에 이 조각을 AND로 붙인다.
  이유: 계획 답 ⑤⑦. 한 규칙을 SQL과 Rust 두 군데에 따로 쓰면 어긋난다. 버린 대안: 타임라인에만 적용
- 행 단위로 거르는 곳(`keyword_performance`, `stat_words`)은 조회 SQL에 `stocks`를 붙이고 같은 조각으로 "숨김 아님" 칸을 더 읽는다(`CASE WHEN <조각> THEN 1 ELSE 0 END`). 통과는 `meets_display_rule(관련도, 영향도) && 숨김 아님`이다. `meets_display_rule`의 서명과 뜻(관련도·영향도)은 그대로 둔다.
- 평가 실행 요약 `relevant`는 지금처럼 관련도·영향도만으로 센다.
  이유: 기록용 수치라 의미를 바꾸지 않는다. 시장 평가가 나중에 생기면 결과가 달라져 저장 시점에 확정할 수 없다.
- 이 작업은 성격 숨김(사진·영상·뉴스 모음)을 넣지 않는다. `article-nature`가 같은 조각에 더한다.
- 조각은 `stocks.deleted_at` 조건을 이미 가진 조회에 붙는다. 시장 항목의 `deleted_at`은 하위 조회 안에서 따로 본다.

## 체크포인트 (각각 검사 통과 후 커밋)
1. SQL 조각과 타임라인 → `timeline/read_tests` 새 시험 통과
2. 토픽 수·키워드 통과·통계 낱말 → 새 시험 통과, `/check` 전체 통과

## 제약
- 저장된 평가는 고치지 않는다(지난 기사도 다시 평가하지 않음, 결정 ⑩).

## 알려진 함정
- `read_timeline`은 기사를 200개씩 훑어 카드로 묶는다. 조건은 SQL에 넣어야 "더 보기" 개수가 맞는다.
- 리뷰(2026-11-14, stock-subject): 가짜 평가의 `bystander` 기본값이 다르다. `test_support::fake_decision()`·E2E 가짜 TypeSafe로 저장한 종목 평가는 0.1(지난 평가 규칙을 받지 않음), SQL로 직접 넣은 평가 줄은 NULL(지난 평가 규칙을 받음). 시험 준비가 어느 쪽인지 보고 깨지는 시험을 고친다.

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고) + 타임라인 조회가 눈에 띄게 느려짐(시험 DB 기준 이전의 2배 이상)
