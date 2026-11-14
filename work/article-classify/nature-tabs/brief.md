# nature-tabs: 제목 앞 성격 태그와 두 탭
모델: Sonnet / effort: medium | 선행: article-nature 통과

## 목표
타임라인에 "사실"·"해석·전망·기타" 두 탭을 두고, 카드 제목 앞에 성격 태그(사실·해석·전망·기타)를 붙인다. 태그가 여럿인 기사는 태그마다 그 탭에 나온다. 성격이 아직 없는 기사는 사실 탭에 태그 없이 나온다.

## 완료 조건
- `timeline/read_tests` 새 시험 통과:
  - 사실 탭: 사실 태그 기사와 성격 NULL 기사가 보이고, 해석만인 기사는 안 보임
  - 해석·전망·기타 탭: 해석·전망·기타 태그 기사가 보이고, 사실만인 기사와 NULL 기사는 안 보임
  - 사실+전망 기사는 두 탭 모두에 보임
  - 같은 소식 묶기는 그 탭 조건을 넘은 기사끼리만, 카드 태그는 묶인 기사 태그의 합
  - 탭과 종목·주제·영향 필터가 함께 걸림, "더 보기" 개수가 맞음
- `web/timeline_tests` 새 시험 통과: `?nature=view` 읽기(모르는 값은 사실 탭), 탭 링크와 필터·주가 띠·토픽 카드 링크가 서로의 값을 지킴, 태그가 제목 앞에 보임
- E2E `timeline.spec.ts` 새 시험 통과(가짜 TypeSafe 성격 규칙 사용): "[포토]" 기사는 어느 탭에도 없음, "전망" 기사는 해석·전망·기타 탭에 "전망" 태그로, 일반 기사는 사실 탭에 "사실" 태그로, 탭을 바꿔도 항목 필터가 남음, 손님 보기에서도 같음
- `/check` 전체 통과(E2E 포함)
- 휴대폰(390px)·PC(1280px) 두 탭 캡처를 직접 보고 인계에 경로

## 수정 가능 범위
- `rust/ripple/src/server/timeline/read.rs`
- `rust/ripple/src/server/timeline/read_tests.rs`
- `rust/ripple/src/web/timeline.rs`
- `rust/ripple/src/web/timeline_tests.rs`
- `rust/ripple/templates/timeline/`
- `e2e/timeline.spec.ts`
- `e2e/guest.spec.ts`
- `e2e/helpers.ts`
- `README.md`(타임라인 설명 줄만)
- `work/article-classify/nature-tabs/`

## 볼 위치
- `timeline/read.rs`의 `TimelineFilter`, `parse_timeline_params`, `read_timeline`(기사 훑기·같은 소식 묶기), `TimelineCard`
- `web/timeline.rs`의 `timeline_href`, `filters_view`, `price_view`, `topic_card_view`, `card_view`, `CardView`
- `templates/timeline/body.html`(주가 띠 → 지금 시장 이슈 → 필터 칩 순서), `card.html`, `ui/badge.html`
- `server/db/schema.rs`의 `NatureTag`와 태그 고르기 함수(schema 작업)
- `e2e/mock-services.mjs`의 성격 답 규칙(article-nature 작업)

## 미리 정한 판단
- 탭은 `TimelineFilter`의 새 값(사실·보기 둘, 기본 사실)이다. 주소 `nature=view`일 때만 해석·전망·기타 탭이고, 사실 탭은 주소에 값을 남기지 않는다. 모든 링크는 지금처럼 `..*filter`로 탭 값을 지킨다.
  이유: 계획 답 ④
- 탭 조건은 SQL이다. 임계값은 `NATURE_THRESHOLD`다.
  - 사실 탭: `(articles.nature_at IS NULL OR articles.nature_fact >= ?)`
  - 해석·전망·기타 탭: `articles.nature_at IS NOT NULL AND (articles.nature_analysis >= ? OR articles.nature_forecast >= ? OR articles.nature_opinion >= ? OR (articles.nature_fact < ? AND articles.nature_analysis < ? AND articles.nature_forecast < ?))`
  기사 페이지 조회와 같은 소식 후보 조회에 같이 건다.
  이유: 계획 세부 ④, 결정 ⑰(태그마다 그 탭). 4A(성격 없음은 사실 탭)
- 주가 띠·지금 시장 이슈·토픽 기사 수는 탭과 상관없이 지금과 같다.
- 탭 모양: 지금 시장 이슈 아래, 필터 칩 위에 `<nav aria-label="기사 성격" data-testid="nature-tabs">`를 둔다. 링크 두 개 "사실"·"해석·전망·기타"를 넣고, 지금 탭은 `aria-current="page"`와 아래 테두리 강조로 표시한다. 두 링크는 한 줄에 같은 너비로 놓는다. htmx 동작은 필터 칩과 같다(`#timeline-body` 교체, 주소 남김).
- 태그 모양: 제목 링크 앞에 작은 테두리 글자 칩(`text-[11px]`, 회색, `data-testid="nature-tag"`)을 사실·해석·전망·기타 순서로 둔다. 태그가 없으면 칩도 없다. 영향 배지와 겹치지 않게 색은 회색 하나로 한다.
- 빈 화면 글자는 지금 규칙 그대로다.

## 체크포인트 (각각 검사 통과 후 커밋)
1. 탭 필터·SQL·묶기·태그 합 → `timeline/read_tests` 새 시험 통과
2. 주소·링크·템플릿 → `web/timeline_tests` 새 시험 통과
3. E2E·캡처 → `/check` 전체 통과, 캡처 확인

## 제약
- 손님 보기 상한(`GUEST_TIMELINE_MAX`)과 권한 규칙은 바꾸지 않는다.
- `docs/`는 고치지 않는다. 명세에 넣을 것(FR-7 탭·태그)은 인계에 적는다.

## 알려진 함정
- 같은 소식 묶기는 SQL 밖에서 한다. 묶는 후보에도 탭 조건을 걸지 않으면 다른 탭 기사가 "외 N곳"에 섞인다.
- 리뷰(2026-11-14, schema): 태그 고르기는 `server/db/schema.rs`의 `classify_nature(&NatureProbs) -> (Vec<NatureTag>, bool)`, 화면 글자는 `NatureTag::label()`이다. 성격이 없는 기사(`nature_at` NULL)는 이 함수에 넣지 말고 태그 없이 둔다(넣으면 Other가 붙는다).

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고)
