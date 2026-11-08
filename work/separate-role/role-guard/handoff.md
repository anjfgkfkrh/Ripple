# role-guard 인계
커밋: cd8327a | 상태: 완료

## 검사 결과 (실행한 명령과 출력 숫자)
| 명령 | 출력 숫자 |
|---|---|
| cargo fmt --check | 통과(0) |
| cargo clippy --all-targets -- -D warnings | 경고·오류 0 |
| cargo test | 618 통과, 0 실패 |
| ./build.sh --release | 성공 |
| pnpm lint / pnpm typecheck | 오류 0 / 오류 0 |
| pnpm test | 1 파일, 7 통과 |
| pnpm test:e2e | 39 통과, 5 건너뜀(모바일 설계상), 0 실패 |
| pnpm format:check | 실패 1건: 미추적 `.playwright-mcp/page-2026-11-08T07-30-59-027Z.yml` 하나뿐(이 작업과 무관, 지우지 않음) |

커밋: 9d4e548(분류·noindex·Access), ca960ab(/unlock), 258aef3(레이아웃), cd8327a(E2E).

## 브리프에 없던 결정
- 기대값 변경(Rust, 모두 GUEST_VIEW 기본 켬 때문):
  - `guard_tests.rs` `pages_redirect_to_login_with_next`, `forged_cookie_is_blocked`: `start_web_guest_off()`으로 옮김(손님 GET `/`는 이제 200)
  - `stocks_tests.rs` `stocks_page_needs_a_login`: `start_web_guest_off()`으로 옮김. `stock_pages_need_a_login_for_posts_too`: 손님 POST 기대 307 → 403
  - `timeline_tests.rs` `set_read_and_timeline_need_a_login`: GET `/`는 guest_off로, 손님 POST `/articles/read` 기대 307 → 401
- 기대값 변경(E2E): `auth.spec.ts` "미로그인으로 홈에 오면 로그인 화면으로 보낸다" → "손님은 홈을 보고, 상태로 오면 로그인 화면으로 보낸다"(`/status` 이동, 손님 `data-access`·로그아웃 없음 확인 추가). "로그인하면 가려던 페이지로 가고, 로그아웃하면 다시 막힌다"는 확인 주소를 `/status`로(`?next=%2Fstatus` 확인). `helpers.ts` `login()`은 `/login?next=<경로>`로 바로 감.
- 시험 도우미(`web.rs` test_support) 추가: `start_web_guest_off()`, `TestWeb::expired_edit_cookie()`(쿠키를 받은 뒤 clock +31분).
- `budget_banner(state)`는 그대로 두고 `budget_banner_for(state, &Access)`를 더했다(손님이면 None). 손님이 닿을 수 있는 화면(타임라인·종목 페이지·404)만 이것을 쓰고, 상태·설정 페이지는 회원 전용이라 기존 함수와 `member: true`를 쓴다.
- 템플릿: 각 페이지 구조체에 `member: bool` 필드를 더했다. `base.html`에 `{% block body_attrs %}`를 두고 `app_layout.html`이 `data-access`를 채운다(로그인 화면은 비움).
- 가드: 수정 권한 부족은 `Need::Edit`이 GET이어도 같은 403 응답을 쓴다. `Need`·`required`는 `guard.rs`에 `pub`. OPTIONS는 Member.
- `check_password()`(login.rs)가 `/login`·`/unlock` 공통(차단 확인 → 검사 → 실패 기록/성공 쿠키).
- 범위 밖: 없음

## 발견한 것 (함정, 실패 교훈, 기존 코드의 버그)
- `pnpm format:check`는 미추적 `.playwright-mcp/*.yml` 때문에 실패한다(브리프 예고대로).
- `Need::Guest`는 `/stocks/*` GET 전부라 `/stocks/markets`·`/stocks/topics/list`도 손님이 본다(브리프 규칙대로).
- 손님 `/stocks` 화면은 아직 등록/삭제 폼을 그대로 보인다(JS·화면 처리는 unlock-ui 몫).

## 상위로 올린 것
없음

## 다음 작업에 영향
- 비밀번호 창은 403 + `HX-Trigger: unlock-required`(htmx) 또는 403 글자(fetch)를 받고 `POST /unlock`(폼 `password`)을 부르면 된다. 성공 204 + 새 쿠키, 실패 401 `{"error": 문구}`.
- 손님 `POST /articles/read`는 401 JSON이다. 모든 응답에 `X-Robots-Tag`가 붙는다.
- 핸들러는 `Extension<Access>`로 접근 정보를 읽는다.
