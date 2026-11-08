# guest-limits 인계
커밋: a3faf34 | 상태: 완료

## 검사 결과 (실행한 명령과 출력 숫자)
| 명령 | 출력 숫자 |
|---|---|
| cargo fmt --check | 출력 없음 |
| cargo clippy --all-targets -- -D warnings | 경고 0 |
| cargo test | 629 통과, 0 실패 |
| ./build.sh --release | 오류 0 |
| pnpm format:check | 실패 1개 파일 (`.playwright-mcp/page-2026-11-08T07-30-59-027Z.yml`, 미추적, 이 작업과 무관) |
| pnpm lint / typecheck | 오류 0 / 오류 0 |
| pnpm test | 7 통과 |
| pnpm test:e2e | 39 통과, 0 실패, 5 건너뜀(모두 mobile) |

## 브리프에 없던 결정
- 범위 안 구현 선택: `WebState.db`를 `Arc<WebDb>`로 바꾸고 `WebDb::read`(async, spawn_blocking)를 둠. `budget_banner`·`budget_banner_for`·`reload`·`collection_panel`·`topics_panel`이 async가 됨. 호출한 곳은 `with` 열 곳(layout 1, timeline 1, stocks 1, status 4, settings 3)
- `JoinError`(panic)는 "DB 읽기 작업이 중단되었습니다."로 돌려줌. panic 뒤 잠금은 기존대로 "DB 잠금이 깨졌습니다."
- 제한기는 `server/auth/guest_limit.rs`(`GuestRateLimiter`, `check(key, now) -> Result<(), 남은 초>`)에 새 파일로 둠(범위 `server/auth/`). 키가 가득 찼을 때 `Retry-After`는 가장 오래된 창이 끝날 때까지 남은 초
- 손님 최대 개수: `parse_timeline_params` 4번째 인자 `max_limit`, `build_body`에 `guest: bool` 인자를 더함. 회원 쪽 "더 보기"는 그대로(600에서도 링크 유지), 손님만 `limit < 90`일 때 링크를 그림
- 키워드 실패 이유: `KeywordsView.failed_lead` 필드를 더하고 손님이면 `StocksData::hide_failure_reasons()`로 `failed_text`를 앞 문장으로 바꿈. `stocks::list`·`markets`·`topic_list`가 `Extension<Access>`를 받음(`reload_for`)
- 범위 밖 변경 없음

## 발견한 것 (함정, 실패 교훈, 기존 코드의 버그)
- 손님 `/stocks` 폴링은 상한에 닿는다: 키워드·트리를 만드는 동안 `/stocks/list`·`/stocks/markets`·`/stocks/topics/list`가 각각 `every 3s`(템플릿 `stocks/list.html`·`markets.html`·`topic_list.html`)라 합쳐 1분에 60번이고, 화면을 연 요청 1번이 더해져 61번째에서 429. 429를 받은 폴링은 htmx가 바꾸지 않고 계속 시도함. 손님 폴링을 세는 규칙은 혼자 바꾸지 않았다(상위로 올림)
- `pnpm format:check` 실패는 미추적 `.playwright-mcp/page-*.yml` 때문. 지우지 않음

## 상위로 올린 것
- 위 손님 `/stocks` 폴링 429 문제: 선택지는 폴링 조각을 세지 않기, 한도를 올리기, 손님은 폴링 주기를 늘리기 등. 결정 필요

## 다음 작업에 영향
- 손님 GET E2E가 한 시험에서 60번을 넘으면 429가 난다(이번 E2E는 통과)
