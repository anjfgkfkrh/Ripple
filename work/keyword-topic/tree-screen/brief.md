# tree-screen: 트리 화면 실적, 별칭 보기·지우기, 설정 이름·범위·기본값
모델: Sonnet / effort: medium | 선행: tree-weekly 통과

## 목표
트리 화면에서 키워드마다 Jev 점수 대신 최근 4주 걸린 기사 수·통과 수를 보인다. 종목 화면에 별칭을 보이고 지울 수 있게 한다. 설정 화면은 이름·범위·기본값을 17단계에 맞춘다.

## 완료 조건
- `stocks_tests`(web) 새 시험 통과:
  - 키워드 줄에 "기사 N · 통과 M"(`data-testid="keyword-stats"`)이 나오고 "관련 … · 영향 …"은 나오지 않음
  - 종목에 별칭 칩(`data-testid="stock-alias"`)이 나옴
  - 주인에게만 별칭 지우기 버튼이 보이고 손님에게는 안 보임
  - 지우기 POST가 worker를 거쳐 그 종목 조각을 다시 그림
- worker 시험 통과: 별칭 지우기 경로가 `remove_alias`를 부르고 없는 종목·없는 별칭에 맞는 응답
- `settings_form_tests` 새 시험 통과: 깊이는 1~2만 받음(3이면 오류), 기본값은 깊이 2·주기 주 1회·영향도 표시 기준 5·토픽을 붙일 기준 6·5
- 고친 E2E 통과: `e2e/keywords.spec.ts`(실적 표시, 별칭 칩 보이기와 지우기), `e2e/settings.spec.ts`(새 이름·기본값)
- `/check` 전체 통과, 종목 화면(트리 펼침·별칭)과 설정 화면을 데스크톱·휴대폰 폭으로 캡처해 인계에 경로를 적음

## 수정 가능 범위
- `rust/ripple/src/web/`
- `rust/ripple/templates/stocks/`
- `rust/ripple/templates/settings/`
- `rust/ripple/static/app.js`
- `rust/ripple/src/worker/server.rs`
- `rust/ripple/src/worker/`(경로 시험 파일)
- `rust/ripple/src/server/worker_client.rs`
- `rust/ripple/src/server/worker_client_tests.rs`
- `rust/ripple/src/server/keywords/read.rs`
- `rust/ripple/src/server/keywords/read_tests.rs`
- `rust/ripple/src/server/settings.rs`
- `rust/ripple/src/server/settings_tests.rs`
- `rust/ripple/src/server/settings_form.rs`
- `rust/ripple/src/server/settings_form_tests.rs`
- `e2e/keywords.spec.ts`
- `e2e/settings.spec.ts`
- `e2e/mock-services.mjs`
- `README.md`(설정·키워드 설명이 있는 곳만)

## 볼 위치
- `templates/stocks/keywords.html`(점수 `keyword-scores` 자리), `templates/stocks/item.html`, `templates/stocks/regen_form.html`(POST 폼·htmx 모양)
- `web/stocks.rs`의 키워드 다시 만들기 처리(worker 호출과 조각 다시 그리기), `web/guard.rs`(GET 아닌 요청은 수정 권한)
- `worker/server.rs`의 `route`(`/stocks/{id}/keywords`)
- `server/keywords/read.rs`, tree-weekly의 키워드 실적 함수
- `server/settings.rs`의 `KEYWORDS_DEPTH`·`KEYWORDS_REGENERATE_EVERY`·`EVALUATION_MIN_MAGNITUDE`, `server/settings_form.rs`의 깊이 검사, `templates/settings/`의 "키워드 남길 관련도·영향도" 칸
- `docs/DECISIONS.md` 1장 "유료 확인 뒤 결정" ②⑥⑦, "17단계 `keyword-topic` 계획 답" ③④, "17단계 계획 때 정한 세부" ②③⑩

## 미리 정한 판단
- 실적은 tree-weekly의 실적 함수를 최근 28일로 불러, 모든 키워드(구조·토픽 가지) 줄 끝에 `기사 {걸린 수} · 통과 {통과 수}`를 작은 회색 글씨로 보인다. 걸린 수가 0이어도 보인다.
  이유: 결정 ⑥, 세부 ③(4주).
- 별칭은 종목(kind=stock) 이름 줄 아래에 "별칭" 머리말과 칩으로 보인다. 별칭이 없으면 줄을 숨긴다. 칩마다 주인에게만 지우기(×) 버튼을 둔다. 버튼은 확인창 없이 바로 지우고 그 종목 조각을 다시 그린다. 고유어는 화면에 보이지 않는다.
  이유: 결정 ②(보여 주고 지울 수 있게). 확인창은 "다시 만들기"처럼 되돌리기 쉬운 동작이라 뺀다. 버린 대안: 고유어도 보이기(결정에 없음)
- 경로: web `POST /stocks/{id}/aliases/remove`(폼 칸 `alias`) → worker `POST /stocks/{id}/aliases/remove`(JSON `{"alias": …}`). 응답은 다시 만들기와 같은 방식으로 조각을 돌려준다. 수정 권한 검사는 기존 guard 규칙 그대로 받는다(허용 목록에 넣지 않음).
- 설정: 칸 이름 "키워드 남길 관련도(0~10)"·"키워드 남길 영향도(0~10)"를 "토픽을 종목에 붙일 관련도(0~10)"·"토픽을 종목에 붙일 영향도(0~10)"로 바꾼다. 키 이름은 그대로다. 깊이 입력은 1~2. 기본값은 `KEYWORDS_REGENERATE_EVERY = "weekly"`, `EVALUATION_MIN_MAGNITUDE = 5.0`이다. 저장된 깊이가 3·4이면 화면에는 2로 보이고, 저장할 때 2가 된다.
  이유: 사용자 답 3B·4A, 결정 ⑦.
- 키워드 요약 줄의 "깊이 N"은 실제 트리의 가장 깊은 단계로 보인다.

## 체크포인트 (각각 검사 통과 후 커밋)
1. 트리 화면 실적 → `stocks_tests` 실적 시험 통과
2. 별칭 칩과 지우기(web·worker·worker_client) → `stocks_tests`·worker 시험 통과
3. 설정 이름·범위·기본값 → `settings_form_tests`·`settings_tests` 통과
4. E2E·README·캡처 → `/check` 전체 통과

## 제약
- 바꿔도 되는 기대값은 `e2e/settings.spec.ts`의 칸 이름·기본값(영향도 6 → 5, 깊이 범위), `e2e/keywords.spec.ts`의 점수 자리, `settings_tests`·`settings_form_tests`의 기본값·깊이 범위뿐이다.
- `docs/`는 고치지 않는다(인계에 적음). 실제 API를 부르지 않는다.

## 알려진 함정
- 손님 화면(16단계)은 수정 버튼을 숨기고, 눌러도 비밀번호 창이 뜬다. 손님 E2E(`e2e/guest.spec.ts`)가 깨지지 않게 버튼에 기존 수정 버튼과 같은 표시를 쓴다.
- 앱 전체 설정을 바꾸는 E2E는 데스크톱에서만 차례로 돌고 바로 되돌린다(DEVELOPMENT 4장).
- 리뷰(2026-11-10, tree-full): `tree.rs`의 `MAX_DEPTH`는 이미 2라 `clamp_depth`(생성·`read.rs` 공용)는 1~2로 자른다. 남은 것은 `settings.rs` 범위·설정 화면이다. 새로 만든 키워드는 관련도·영향도가 NULL이라 지금 점수 자리(`keyword-scores`)는 비어 보이고, E2E 가짜 AI 트리는 7개(별칭 `가짜별칭`, 고유어 `반도체·데이터센터·HBM·메모리·GPU`)다.

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고) + 손님 화면의 동작을 바꿔야 할 때
