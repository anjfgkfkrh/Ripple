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
- `e2e/collect.spec.ts`(상태 화면 문구 기대값만)
- `e2e/mock-services.mjs`
- `README.md`(설정·키워드 설명이 있는 곳만)
- 기본값 변경으로 깨지는 기존 시험(기대값·시험 입력만, 제품 코드는 안 됨): `rust/ripple/src/server/evaluate/run_tests.rs`, `rust/ripple/src/server/keywords/performance_tests.rs`, `rust/ripple/src/server/schedule/plan_tests.rs`, `rust/ripple/src/server/schedule/scheduler_tests.rs`, `rust/ripple/src/server/timeline/read_tests.rs`, `rust/ripple/src/web/timeline_tests.rs`

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
- 바꿔도 되는 기대값은 `e2e/settings.spec.ts`의 칸 이름·기본값(영향도 6 → 5, 깊이 범위), `e2e/keywords.spec.ts`의 점수 자리, `settings_tests`·`settings_form_tests`의 기본값·깊이 범위, 그리고 "수정 가능 범위"에 더한 기존 시험 6개 파일의 기본값 의존 부분뿐이다. 이 6개 파일은 시험의 뜻이 기본값 자체를 확인하는 것이면(예: `plan_tests.rs`의 기본 주기, `read_tests.rs`의 `page.min_magnitude`, `web/timeline_tests.rs`의 "영향도 N점 이상" 문구) 기대값을 새 기본값(weekly·5)으로 고치고, 기본값과 상관없는 동작을 확인하는 시험이면(스케줄러 다시 만들기 횟수, 표시 규칙 경계 등) 시험 준비에서 옛 값(`daily`·6)을 설정으로 직접 써서 원래 시나리오를 지킨다. 제품 코드는 바꾸지 않는다.
- `docs/`는 고치지 않는다(인계에 적음). 실제 API를 부르지 않는다.

## 알려진 함정
- 손님 화면(16단계)은 수정 버튼을 숨기고, 눌러도 비밀번호 창이 뜬다. 손님 E2E(`e2e/guest.spec.ts`)가 깨지지 않게 버튼에 기존 수정 버튼과 같은 표시를 쓴다.
- 앱 전체 설정을 바꾸는 E2E는 데스크톱에서만 차례로 돌고 바로 되돌린다(DEVELOPMENT 4장).
- 리뷰(2026-11-10, tree-full): `tree.rs`의 `MAX_DEPTH`는 이미 2라 `clamp_depth`(생성·`read.rs` 공용)는 1~2로 자른다. 남은 것은 `settings.rs` 범위·설정 화면이다. 새로 만든 키워드는 관련도·영향도가 NULL이라 지금 점수 자리(`keyword-scores`)는 비어 보이고, E2E 가짜 AI 트리는 7개(별칭 `가짜별칭`, 고유어 `반도체·데이터센터·HBM·메모리·GPU`)다.

- 리뷰(2026-11-10, tree-weekly): 실적 함수는 `server/keywords/performance.rs` `keyword_performance(conn, stock_id, since)`이고 표의 키는 `tree.rs` `normalize_key`(공백 빼고 소문자)한 키워드 글자다. 키워드 줄 글자도 `normalize_key`로 찾아야 맞는다. 정기 점검은 이제 주기와 상관없이 매일 06:00에 확인하고 대상만 주기로 고르므로, 설정 화면 주기 칸 설명이 "다시 만들기"를 말하면 "정기 점검"에 맞게 고친다.

- 사람 결정(2026-11-10, 시도 2 진단 뒤 답 "A"): 상태 화면 문구 `web/status.rs` `keywords_schedule_text`를 "매일 06:00 확인(주 1회 점검)"처럼 실제 동작(주기와 상관없이 매일 확인, 주기가 지난 종목만 점검)에 맞게 고치고 `e2e/collect.spec.ts:30` 기대값을 그 문구로 맞춘다. DECISIONS.md 1장 "17단계 tree-screen 상태 화면 문구". 남은 일: 이 수정, `/check` 전체, 캡처 직접 보기

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고) + 손님 화면의 동작을 바꿔야 할 때

- 시도 1 결과(마지막 커밋 f968207): 체크포인트 1~3은 끝났고 커밋됐다(실적 표시, 별칭 칩·지우기 web·worker·worker_client, 설정 이름·범위·기본값). 재시도는 이어서 기존 시험 12개(`evaluate::run` 1, `keywords::performance` 1, `schedule::plan` 2, `schedule::scheduler` 4, `timeline::read` 2, `web::timeline` 2; `cargo test` 695 통과·12 실패)를 위 "제약"대로 고친 뒤 체크포인트 4(E2E·README·`/check`·캡처)를 한다. 배제한 방법: 기본값을 되돌리는 것(사용자 결정 3B·4A·⑦), 제품 코드에서 기본값을 우회하는 것.
- 진단(2026-11-10, 시도 1, Sonnet/medium): 브리프 빈틈 — 기본값(weekly·영향도 5) 변경이 기본값에 기대는 범위 밖 기존 시험 12개를 깨는 것을 브리프가 예상하지 못해 수정 범위·기대값 규칙이 없었다 → 브리프 빈틈
- 시도 2 결과(마지막 커밋 57c5b86): 기존 시험 12개 고침(`cargo test` 707 통과), 체크포인트 4의 E2E(`keywords`·`settings`)·README 끝. 남은 것은 `e2e/collect.spec.ts:30`(`/매일 06:00 · (곧 시작|다음 )/`)이 기본 주기 weekly에서 "매주 월요일 06:00 · 곧 시작"을 받아 실패하는 것, 그 뒤 `/check` 전체와 캡처 직접 보기. 원인 확인: `web/status.rs` `keywords_schedule_text`가 weekly면 "매주 월요일 06:00"을 쓰지만 tree-weekly 뒤 `schedule/plan.rs` `keyword_plan`은 주기와 상관없이 매일 06:00에 확인(`next_at`도 매일)하므로 상태 화면 문구가 실제 동작과 어긋난다(tree-weekly가 남긴 표시 버그). 배제한 방법: 기본값 되돌리기, 범위 밖 파일 혼자 고치기.
- 진단(2026-11-10, 시도 2, Sonnet/medium): 브리프 빈틈 — 기본 주기 변경이 범위 밖 `e2e/collect.spec.ts:30`을 깨고, 그 밑에는 상태 화면 문구(`web/status.rs` `keywords_schedule_text`의 "매주 월요일 06:00")가 tree-weekly 뒤 매일 확인 동작과 어긋나는 문제가 있다. 같은 단계 브리프 빈틈 두 번째·하위 보고 2회라 사람에게 올림. 사람이 정할 것: 상태 화면 문구를 "매일 06:00 확인(주 1회 점검)"처럼 고치고 `web/status.rs`·`e2e/collect.spec.ts`를 범위에 더할지(추천), 아니면 collect.spec 기대값만 고칠지 → 사람에게
