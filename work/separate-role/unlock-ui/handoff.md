# unlock-ui 인계
커밋: 79bb840 | 상태: 완료

## 검사 결과 (실행한 명령과 출력 숫자)
| 명령 | 출력 숫자 |
|---|---|
| `cargo fmt --check` | 오류 0 |
| `cargo clippy --all-targets -- -D warnings` | 경고 0 |
| `cargo test` | 629 통과, 0 실패 |
| `pnpm format:check` | 실패 1: 미추적 `.playwright-mcp/page-2026-11-08T07-30-59-027Z.yml`뿐(이 작업과 무관, 지우지 않음) |
| `pnpm lint` | 오류 0 |
| `pnpm typecheck` | 오류 0 |
| `pnpm test` | 1 파일, 7 통과 |
| `./build.sh --release` | 성공 |
| `pnpm test:e2e` | 49 통과, 5 건너뜀, 0 실패 |
| `playwright test e2e/guest.spec.ts` | 10 통과(desktop 5 + mobile 5) |

## 브리프에 없던 결정
- 체크포인트 1·2는 한 번에 구현해 커밋 하나(5a49174)로 묶었다. 3은 79bb840.
- 시험 하나 더함: "삭제 확인창 위에서도 비밀번호 창이 뜬다"(함정 1 확인용), "취소하면 창이 닫히고…". 비밀번호 창 제출 단추는 `ui/button.html`의 `primary`를, 취소는 `dialog_cancel`을 썼다(새 부품 없음).
- `lockButton`이 원래 글자를 `data-idle-label`에 저장하고, 403이면 `unlockButton`으로 단추와 글자를 되돌린다.
- 다시 보낸 요청도 403이면 그 폼 바로 뒤에 `<p role="alert" data-unlock-failed>수정 권한을 얻지 못했습니다.</p>`를 붙이고, 다음 요청 시작 때 지운다.
- 폼이 아닌 요소는 `htmx.trigger(elt, "click")`로 다시 보낸다(지금 htmx 수정 요소는 모두 폼이라 쓰이지 않음).
- 비밀번호 창 제목은 "비밀번호 확인"(브리프에 제목 지정 없음).
- 손님 시험용 종목·기사는 `timeline.spec.ts`를 건드리지 않으려고 `guest.spec.ts` 안에 따로 넣었다(`손님시험<프로젝트>`).
- `.env.example`은 이미 `GUEST_VIEW`·`EDIT_UNLOCK_MINUTES`가 있어 고치지 않았다.
- Cloudflare 절: 무료 요금제 값(규칙 1개, 10초, 10초, IP)은 `developers.cloudflare.com/waf/rate-limiting-rules/` 표에서, 메뉴 이름은 `.../create-zone-dashboard/`에서 가져왔다. "모든 경로" 조건 값은 문서에 없어 "대시보드에서 확인"으로 적었다.
- README 읽음 설명에 "(손님은 그 브라우저에만 저장)"을 더했다.

## 발견한 것 (함정, 실패 교훈, 기존 코드의 버그)
- 손님이 `/unlock`에 성공해도 화면의 `data-access`는 `guest`로 남는다(새로 고칠 때까지). 그동안 읽음은 계속 `localStorage`에 저장되고, 새로 고치면 주인 화면이 되어 서버 읽음을 보이므로 그 읽음은 사라져 보인다. 로그아웃 단추·예산 배너도 새로 고칠 때까지 없다. 시험은 이 상태를 고정하지 않는다.
- `pnpm format:check`는 `.playwright-mcp/*.yml` 때문에만 실패한다.
- 비밀번호 창은 `app_layout.html`에 있어 로그인 화면(`base.html`)에는 없다. `app.js`는 창이 없으면 해당 코드를 건너뛴다.
- 삭제 확인창 안 폼이 403을 받아도 확인창은 열린 채 남고, 비밀번호 창이 그 위에 뜬다(시험으로 확인).

## 상위로 올린 것
없음

## 다음 작업에 영향
- 손님 읽음 저장소 키는 `ripple:reads`(기사 id 배열, 최근 3000개).
- 손님→주인 전환 직후 화면 갱신(`data-access` 바꾸기 또는 새로 고침)은 하지 않았다. 필요하면 결정 필요.
