# role-guard: 손님·회원·수정 권한으로 요청 막기
모델: Sonnet / effort: medium | 선행: access-token 통과

## 목표
guard가 요청마다 `Access`를 만들어 요청 확장(extensions)에 넣고, 경로·메서드로 손님 허용 / 회원 필요 / 수정 권한 필요를 가른다. 기본은 닫힘이다. 비밀번호 창이 부를 `POST /unlock`, 모든 응답의 `noindex`, 상단 로그아웃·예산 배너의 손님 처리를 더하고, 바뀐 동작에 맞게 기존 E2E를 고친다.

## 완료 조건
- `guard_tests.rs` 새 시험 통과:
  - 손님 GET `/`·`/stocks`·`/stocks/list` 200
  - 손님 GET `/status`·`/settings`·`/status/system` → `/login?next=` 이동(htmx면 401 + `HX-Redirect`)
  - `GUEST_VIEW=off`면 손님 GET `/`도 `/login`으로 이동
  - 손님·수정 권한 지난 회원이 POST `/settings`·`/stocks/register`·`/status/collect`(htmx) → 403 + `HX-Trigger: unlock-required`
  - 목록에 없는 POST 주소도 손님에게 403(기본 닫힘)
  - 회원(수정 권한 지남) POST `/articles/read` 통과, 손님 POST `/articles/read` 401 JSON(로그인 화면으로 보내지 않음)
  - 모든 응답에 `X-Robots-Tag: noindex, nofollow`
- `login_tests.rs` 새 시험 통과: `POST /unlock` 맞으면 204 + `Set-Cookie`(새 토큰으로 수정 가능), 틀리면 401 JSON `{"error": …}`, 5번 틀리면 맞는 비밀번호도 401(차단 문구), 실패 제한은 `/login`과 같은 기록을 쓴다
- 고친 `e2e/auth.spec.ts`·`e2e/helpers.ts`와 나머지 E2E가 모두 통과한다
- `/check` 전체 통과

## 수정 가능 범위
- `rust/ripple/src/web/`
- `rust/ripple/src/web.rs`
- `rust/ripple/src/server/auth/`
- `rust/ripple/src/test_support.rs`
- `rust/ripple/templates/base.html`
- `rust/ripple/templates/app_layout.html`
- `rust/ripple/templates/budget_banner.html`
- `e2e/helpers.ts`
- `e2e/auth.spec.ts`
- `playwright.config.ts`
- `work/separate-role/role-guard/`

## 볼 위치
- `rust/ripple/src/web/guard.rs`의 `guard`·`is_public`·`login_location`
- `rust/ripple/src/web.rs`의 `web_router`(경로 목록)와 시험 도우미 `start_web`
- `rust/ripple/src/web/login.rs`의 `submit`·`session_cookie_header`
- `rust/ripple/src/web/layout.rs`의 `budget_banner`, `templates/app_layout.html`
- `docs/DECISIONS.md` 1장 "16단계 계획 세부" ③~⑧

## 미리 정한 판단
- 분류 함수는 `fn required(method, path, guest_view) -> Need { Public, Guest, Member, Edit }` 하나로 둔다. 규칙:
  - Public: `/login`(GET·POST), `/unlock`(POST), `/logout`(POST), `/favicon.ico`, `/static/*`
  - Guest: GET·HEAD의 `/`, `/stocks`, `/stocks/*`(단 `guest_view`가 켜졌을 때만, 꺼졌으면 Member)
  - Member: 그 밖의 GET·HEAD·OPTIONS, POST `/articles/read`
  - Edit: 그 밖의 모든 요청
  이유: 새 POST가 생겨도 기본이 수정 권한이다(결정 ③). 버린 대안: 막을 주소를 하나씩 적기
- 권한이 모자랄 때 응답:
  - Member가 필요하고 손님이면, GET·HEAD·OPTIONS는 지금 동작 그대로(`/login?next=`, htmx는 401 + `HX-Redirect`, `/api/`는 JSON 401). GET이 아니면 경로와 상관없이 401 JSON `{"error":"unauthorized"}`
    이유: `fetch` POST가 307로 `/login`에 본문째 다시 보내지는 것을 막는다. 수정 권한 403(`unlock-required`)과 구별되게 401로 둔다
  - Edit가 필요한데 없으면 htmx는 403 + `HX-Trigger: unlock-required`, `/api/`는 JSON 403 `{"error":"unlock_required"}`, 그 밖은 403 글자 "수정하려면 비밀번호가 필요합니다."
  - 손님이 Edit 요청을 해도 로그인 화면으로 보내지 않는다(다음 작업의 비밀번호 창이 받는다)
  - Origin 검사는 지금처럼 맨 앞에 둔다
- `Access`는 `request.extensions_mut().insert(access)`로 넣고, 핸들러·템플릿 조립은 `Extension<Access>`로 읽는다.
  이유: 계정 기능 때 핸들러가 `sub`를 그대로 쓸 수 있다.
- `POST /unlock`은 폼 값 `password` 하나를 받는다. `/login` 성공과 같은 새 토큰 쿠키를 내고 204를 돌려준다. 실패와 차단은 401 JSON `{"error": 문구}`이고, 문구는 `/login`과 같다. 비밀번호 검사·실패 제한 코드는 `/login`과 함께 쓰도록 함수로 뺀다.
  이유: 비밀번호 창은 JS가 fetch로 부르므로 화면 대신 상태 코드가 필요하다. 버린 대안: `/login`이 htmx 조각을 돌려주기(로그인 화면과 섞임)
- `X-Robots-Tag: noindex, nofollow`는 guard에서(통과·거절 모두) 붙인다.
- `app_layout.html`의 로그아웃 폼은 회원에게만 그린다. 메뉴 네 개는 누구에게나 그대로 둔다. 예산 배너(`budget_banner`)는 손님에게 `None`이다. `<body>`(또는 `base.html` 루트)에 `data-access="guest|member"`를 넣는다(다음 작업의 JS가 읽음).
  이유: 결정 ⑥-4(상단 상태 표시 없음)와 ⑧. 상태·설정 링크가 주인의 입구다.
- 손님에게 타임라인 카드의 `read`는 서버가 항상 false로 그린다(주인 읽음을 손님에게 보이지 않음).
- E2E 고치기(기대값 변경 허락):
  - `helpers.ts`의 `login()`은 `"/login?next=" + path`로 바로 가서 비밀번호를 낸다
  - `auth.spec.ts`의 "미로그인으로 홈에 오면 로그인 화면으로 보낸다"는 "손님은 홈을 보고, 상태로 오면 로그인 화면으로 보낸다"로 바꾼다
  - "다른 페이지·API" 시험의 `/some/page?x=1`은 Member 분류라 기대값이 그대로다
  - "로그아웃하면 다시 막힌다"의 확인 주소는 `/status`로 바꾼다
  - 그 밖의 `auth.spec.ts` 시험과 다른 spec의 기대값은 바꾸지 않는다
  - 바꾼 시험 이름은 인계에 모두 적는다

## 체크포인트 (각각 검사 통과 후 커밋)
1. `guard.rs` 분류·응답·`noindex`·`Access` 확장 → `guard_tests` 새 시험 통과
2. `POST /unlock`과 비밀번호 검사 함수 공유 → `login_tests` 새 시험 통과
3. 상단 로그아웃·예산 배너·`data-access`·손님 카드 `read=false` → 해당 Rust 시험 통과
4. `helpers.ts`·`auth.spec.ts` 고치기 → `/check` 전체(E2E 포함) 통과

## 제약
- 위에 적은 E2E 말고는 기존 시험의 기대값을 바꾸지 않는다
- 비밀 값은 로그·응답에 넣지 않는다. 실패 로그는 지금처럼 `client_key`만 남긴다
- 비밀번호 창 JS·손님 읽음 JS는 이 작업에서 만들지 않는다(다음 작업 unlock-ui)

## 알려진 함정
- htmx는 4xx 응답을 바꿔 끼우지 않는다. 403 본문은 화면에 나오지 않으므로 헤더(`HX-Trigger`)로 알린다
- 시험용 web 도우미가 `GUEST_VIEW` 기본(켬)으로 뜨므로, 기존 guard 시험의 "손님 GET `/` → 로그인 이동" 기대는 `GUEST_VIEW=off` 시험으로 옮겨야 한다. 이것은 기대값 변경이므로 인계에 적는다
- 리뷰(2026-11-08, access-token): 시험 도우미 `start_web`은 `guest_view: true`·`edit_unlock_minutes: 30`이 박혀 있다. `GUEST_VIEW=off` 시험은 도우미에 설정을 넘길 길을 더해야 한다(`start_web_with_tunnel`처럼)
- 리뷰(2026-11-08, access-token): `cookie()`는 지금 시계로 새 토큰(수정 권한 30분)을 낸다. "수정 권한 지난 회원"은 `cookie()`를 받은 뒤 `clock`을 30분 이상 넘겨 만든다
- 리뷰(2026-11-08, access-token): `pnpm format:check`가 미추적 `.playwright-mcp/page-*.yml`(playwright MCP 캡처 기록) 때문에 실패한다. 이 작업 변경과 무관하면 지우지 말고 인계 "발견한 것"에 적는다

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고) + 위 목록 밖의 E2E 기대값을 바꿔야 할 때, 분류 규칙에 맞지 않는 경로가 나올 때
