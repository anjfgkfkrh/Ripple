# topic-input: 토픽 갱신 입력 줄이기·새 규칙·범위(scope)·통과 기준 바로잡기
모델: Sonnet / effort: medium | 선행: paid-compare 통과, anchor-schema 통과

## 목표
토픽 갱신에 넣는 제목에서 일본어판을 빼고 같은 소식을 한 줄(`×N`)로 묶는다. 프롬프트에 화제성·추문 제외와 토픽 범위(market/company) 규칙을 넣는다. company 토픽은 토픽 항목·카드 없이 관련 종목 가지만 둔다. 토픽 유지·이전 토픽 목록의 "통과"는 화면 표시 기준으로 센다.

## 완료 조건
- `topics_tests` 새 시험 통과:
  - 입력 줄이기: 가나가 든 제목은 빠짐, 같은 소식은 가장 최근 제목 하나 뒤에 ` ×N`, 하나뿐이면 표시 없음, 순서 유지
  - 프롬프트가 `prompts/topics.json`과 같음
  - 응답의 `scope` 해석(모르는 값은 market)
  - company 토픽: 토픽 항목을 만들지 않음, 이미 있던 항목은 숨김, 관련 종목이 없으면 버림, 관련 종목 가지는 지금처럼 Jev 평가 뒤 붙음, `topics.scope`에 저장
  - company 토픽의 24시간 기사 수는 종목 가지 경로로 셈
  - `topic_article_counts`의 통과 기준이 화면 표시 기준(`evaluation.minRelevance`·`minMagnitude`)
  - 토픽 카드에 company 토픽이 나오지 않음
  - (리뷰가 쓰기로 정했으면) 이전 토픽 줄에 통과 건수
- `/check` 전체 통과(E2E 포함)

## 수정 가능 범위
- `rust/ripple/src/server/topics/`
- `rust/ripple/src/server/prompts/topics.json`
- `rust/ripple/src/server/ai/llm/schemas/topic_refresh.json`
- `rust/ripple/src/server/ai/llm/schemas_tests.rs`
- `rust/ripple/src/server/collect/feed_headlines.rs`(제목 읽기에 필요할 때만)
- `rust/ripple/src/server/timeline/read.rs`(`read_topic_cards`만)
- `rust/ripple/src/server/timeline/read_tests.rs`
- `e2e/mock-services.mjs`(`topic_refresh` 답에 `scope`)

## 볼 위치
- `scripts/analysis/keyword-topic/llm_make.py`: `topic_system("B")`, `TOPIC_SCHEMA_B`(scope 칸), `reduce_titles`, `topic_user`
- `server/topics/refresh.rs` 전체, `server/topics/run.rs`의 `refresh_topics`·`retain_dropped`, `server/topics/store.rs`의 `sync_topic_lenses`·`attach_topic_to_stock`·`topic_article_counts`
- `server/similar_title.rs`의 `title_features`·`is_same_news`
- `server/timeline/read.rs`의 `read_topic_cards`
- `docs/DECISIONS.md` 1장 "키워드 트리·토픽 고도화" ④, "유료 확인 뒤 결정" ④, "17단계 계획 때 정한 세부" ②⑧⑨⑪과 리뷰가 적은 "17단계 토픽 통과 건수" 줄, 2장 "유료 확인"의 "토픽 갱신 줄인 입력" 줄

## 미리 정한 판단
- 시스템 프롬프트는 `topic_system("B")`를 글자 그대로 옮긴다(`MAX_TOPICS` 15). 사용자 메시지는 지금 모양 그대로이고, 제목만 줄인 것을 넣는다. "새 뉴스 제목(최근순, N개)"의 N은 줄인 뒤 줄 수다.
  이유: 실측 문구다(비용 9~25% 감소, 같은 id 9/12·12/13·7/9).
- 입력 줄이기: 지금처럼 최근 1,500개(`MAX_REFRESH_TITLES`)를 읽은 뒤 가나(U+3040~U+30FF)가 든 제목을 빼고, 최근순으로 훑으며 앞서 고른 대표와 `is_same_news`가 맞으면 그 묶음에 센다. 대표는 처음 나온(가장 최근) 제목이고 2건 이상이면 ` ×N`을 붙인다. `MIN_REFRESH_TITLES`(20)는 줄이기 전 제목 수로 본다.
  이유: 계획 세부 ⑨. 같은 소식 기준은 평가 쪽과 같게 둔다. 버린 대안: 줄인 뒤 1,500개를 채우기(입력이 늘어남)
- 형식: `topic_refresh.json` 토픽 항목에 `scope`(`enum ["market", "company"]`, 필수, 설명은 `TOPIC_SCHEMA_B` 그대로)를 더한다. 모르는 값은 market이다.
- company 토픽:
  - `sync_topic_lenses`를 부르지 않는다. 이미 있던 항목은 숨긴다(`deleted_at = 지금`). market으로 바뀌면 지금처럼 항목을 맞춘다.
  - 정리한 related가 비면 그 토픽을 버린다.
  - 관련 종목 가지는 지금처럼 Jev 쌍 평가(`keywords.min*` 기준) 뒤 붙인다.
  - 24시간 기사 수는 붙은 종목의 `article_matches.keyword_path`가 `{종목 표시 이름} → {토픽 이름}`이거나 그 뒤에 ` → `가 이어지는 연결로 센다.
  - 토픽 카드(`read_topic_cards`)에서 뺀다.
  이유: 결정 ④("종목 한 곳 얘기인 토픽은 시장 기준 평가 없이 종목 가지만"), 계획 세부 ⑧.
- `topic_article_counts`의 통과 기준을 `EVALUATION_MIN_RELEVANCE`·`EVALUATION_MIN_MAGNITUDE`로 바꾼다. 토픽 × 종목 쌍을 붙이는 기준은 지금처럼 `KEYWORDS_MIN_*`다.
  이유: 계획 세부 ②(지금은 키워드 기준 6·5로 "화면 통과"를 셈).
- 이전 토픽 줄의 통과 건수: DECISIONS 1장 "17단계 토픽 통과 건수" 줄이 쓰기로 정했으면 줄 끝을 `· 기사 {N}건 · 통과 {M}건{ · 끝남}`으로, 머리줄을 "이전 토픽(id: 이름 — 설명 [검색어] · 24시간 기사 N건 · 통과 M건):"으로 바꾼다. 그 줄이 쓰지 않기로 정했으면 지금 모양 그대로 둔다.

## 체크포인트 (각각 검사 통과 후 커밋)
1. 입력 줄이기와 새 프롬프트·`prompts/topics.json` → 줄이기·프롬프트 시험 통과
2. `scope` 형식·해석·저장과 company 처리 → company 시험, `schemas_tests` 통과
3. 통과 기준·통과 건수·토픽 카드 → 나머지 새 시험 통과
4. E2E 가짜 서버 → `/check` 전체 통과

## 제약
- 바꿔도 되는 기대값은 `prompts/topics.json`과, `topics_tests`에서 통과 기준을 키워드 기준으로 기대하는 시험뿐이다. 그 밖의 기대값은 바꾸지 않는다.
- 실제 API를 부르지 않는다. `docs/`는 고치지 않는다(인계에 적음).

## 알려진 함정
- AI가 이전 토픽 id를 안 돌려주는 경우를 위한 `continue_topics`는 그대로 둔다.
- 새 규칙("추문 제외")은 실측에서 3번 중 1번만 '젠슨 황 불륜' 토픽을 뺐다. 코드로 거르지 않는다.
- 리뷰(2026-11-10, paid-compare): "17단계 토픽 통과 건수"는 보이기로 정함. 실측 창2(이전 토픽 25개)에서 AI가 토픽 id를 모두 빈 문자열로 냈다 — `continue_topics`(이름·검색어로 잇기)가 계속 필요하고, id 없는 응답 시험도 깨지지 않게 둔다.

- 리뷰(2026-11-10, tree-screen): 기본값이 바뀌었다(`EVALUATION_MIN_MAGNITUDE` 5, `KEYWORDS_REGENERATE_EVERY` weekly). 영향도 6을 경계로 쓰던 시험은 tree-screen에서 시험 준비에 6을 직접 써서 지켰다. 새 시험도 경계값을 기본값에 기대지 말고 설정으로 직접 쓴다.

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고) + "17단계 토픽 통과 건수" 줄이 없을 때
