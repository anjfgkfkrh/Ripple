# unlock-ui: 비밀번호 창, 손님 읽음, 안내 문서
모델: Sonnet / effort: medium | 선행: guest-limits 통과

## 목표
수정 권한 없이 수정 동작을 하면 비밀번호 창이 뜨고, 맞으면 원래 요청을 한 번 다시 보낸다. 손님의 읽음 표시는 브라우저에만 저장한다. README·INSTALL·`.env.example`에 손님 보기와 새 환경 변수를 안내한다.

## 완료 조건
- 새 `e2e/guest.spec.ts` 통과(desktop·mobile 프로젝트 모두 해당되면):
  - 손님이 `/`·`/stocks`를 비밀번호 없이 보고, 로그아웃 버튼이 없다
  - 손님이 설정 링크를 누르면 로그인 화면으로 간다
  - 손님이 종목 화면에서 "찾기"를 누르면 비밀번호 창이 뜬다. 틀린 비밀번호는 창 안에 오류를 보인다. 맞는 비밀번호를 넣으면 창이 닫히고 같은 검색이 실행되어 후보가 보인다(입력 값 유지)
  - 비밀번호 창을 거친 뒤 상태·설정 화면이 열린다(주인 로그인이 생김)
  - 손님이 카드 링크·"읽음으로 표시"를 누르면 그 카드가 읽음으로 보이고, 새로 고침 뒤에도 남는다. 서버 읽음(`article_reads`)은 바뀌지 않는다. 주인으로 로그인한 다른 브라우저 문맥에서는 그 카드가 안 읽음이다
- `/check` 전체 통과
- 손님 타임라인, 비밀번호 창(오류 포함) 화면을 데스크톱·휴대폰 크기로 캡처해 작업 폴더에 둔다

## 수정 가능 범위
- `rust/ripple/static/app.js`
- `rust/ripple/templates/`
- `rust/ripple/styles/`
- `rust/ripple/src/web/`
- `e2e/guest.spec.ts`
- `e2e/helpers.ts`
- `e2e/mock-services.mjs`
- `README.md`
- `release/INSTALL.md`
- `.env.example`
- `work/separate-role/unlock-ui/`

## 볼 위치
- `rust/ripple/static/app.js`의 `lockButton`, `htmx:beforeRequest` 처리, 읽음 표시(`saveRead`·`openedLink`), 확인창(`data-dialog-open`)
- `rust/ripple/templates/app_layout.html`, `templates/stocks/delete_form.html`(확인창 안의 htmx 폼)
- `e2e/timeline.spec.ts`의 읽음 시험(카드 고르는 법)
- `docs/DECISIONS.md` 1장 "16단계 계획 세부" ⑤·⑨·⑩

## 미리 정한 판단
- 비밀번호 창은 `app_layout.html`에 네이티브 `<dialog>` 하나로 둔다. 안에는 비밀번호 칸(라벨 "비밀번호"), "확인"·"취소" 단추, 오류 자리(`role="alert"`)가 있고, 설명 글은 "수정하려면 비밀번호를 입력하세요."이다. 지금 있는 확인창과 같은 모양 부품(`ui/`)을 쓴다.
- 흐름: `htmx:responseError`에서 상태 403이고 응답 헤더 `HX-Trigger`에 `unlock-required`가 있으면 그 요청의 원래 요소(`event.detail.elt`)를 기억하고 창을 연다.
  - "확인" → `fetch("/unlock", {method:"POST", body: URLSearchParams{password}})`
  - 204면 창을 닫고 기억한 요소에 `htmx.trigger(elt, "submit")`(폼이 아니면 그 요소의 htmx 이벤트)로 한 번 다시 보낸다
  - 401이면 응답 JSON의 `error`를 창 안에 보인다
  - 다시 보낸 요청도 403이면 창을 다시 열지 않고, 그 폼 근처에 "수정 권한을 얻지 못했습니다."를 보인다
  이유: 결정 ⑤·⑩. 비밀번호 검사와 권한 판단은 서버만 한다. 버린 대안: 수정 칸을 미리 잠그기(사용자가 5B를 고르지 않음)
- 403을 받으면 `lockButton`으로 잠긴 제출 단추를 풀어 둔다(지금은 응답이 바꿔 끼워질 때만 단추가 새로 그려진다).
- 손님 읽음: `document.body.dataset.access === "guest"`면 `saveRead`가 서버 대신 `localStorage` 키 `ripple:reads`(기사 id 배열, 최근 3000개, 넘치면 오래된 것부터 지움)를 고친다. 페이지를 불러올 때와 htmx로 카드가 새로 붙을 때(`htmx:afterSettle`) 카드의 기사 id 중 하나라도 저장돼 있으면 읽음으로 보인다. `localStorage`를 못 쓰면 조용히 화면만 바꾼다(try/catch).
  이유: 결정 ③·⑨. 서버 카드 읽음 규칙(하나라도 읽었으면 읽음)과 같게 한다.
- README 환경 변수 표와 `release/INSTALL.md` 표에 `GUEST_VIEW`·`EDIT_UNLOCK_MINUTES` 줄을 더하고, "손님 보기" 짧은 절을 쓴다. 쓸 내용:
  - 주소를 아는 사람은 누구나 타임라인·종목을 봄
  - 끄는 법
  - 수정할 때 비밀번호 창, 30분
  - 배포 뒤 한 번 다시 로그인
  - 손님은 IP별 1분 60번·최대 90개
  - "선택: Cloudflare 요청 제한 규칙" 절. 무료 요금제 규칙 1개로 모든 경로, IP별 10초에 50번을 넘으면 10초 막기. 메뉴 이름은 Cloudflare 공식 문서(`developers.cloudflare.com/waf/rate-limiting-rules/` 아래 만들기 안내)를 읽고 그대로 쓴다. 문서에 없는 단계는 "대시보드에서 확인"이라고 적는다

## 체크포인트 (각각 검사 통과 후 커밋)
1. 비밀번호 창과 원래 요청 다시 보내기 → `guest.spec.ts`의 비밀번호 창 시험 통과
2. 손님 읽음 `localStorage` → `guest.spec.ts`의 읽음 시험 통과
3. README·INSTALL·`.env.example`, 화면 캡처 → `/check` 전체 통과

## 제약
- 기존 E2E의 기대값은 바꾸지 않는다
- 외부 스크립트·라이브러리를 더하지 않는다(htmx와 `app.js`만)
- `docs/`는 고치지 않는다. 명세와 다른 점은 인계에 적는다

## 알려진 함정
- 종목 삭제 폼은 이미 열린 확인창(`dialog`) 안에 있다. 비밀번호 창은 그 위에 `showModal()`로 열려야 하고, 3초 폴링 건너뛰기 규칙(`dialog[open]`)과 부딪치지 않는지 확인한다
- htmx 2는 4xx 응답을 바꿔 끼우지 않고 `htmx:responseError`를 낸다. `htmx.trigger`로 다시 보낼 때 `htmx:beforeRequest`의 단추 잠금이 다시 걸리는지 확인한다
- 리뷰(2026-11-08, access-token): `pnpm format:check`가 미추적 `.playwright-mcp/page-*.yml`(playwright MCP 캡처 기록) 때문에 실패한다. 이 작업 변경과 무관하면 지우지 말고 인계 "발견한 것"에 적는다
- 리뷰(2026-11-08, role-guard): 손님이 `/unlock`에 성공하면 회원 쿠키를 받지만, 화면의 `data-access`는 새로 고칠 때까지 `guest`다(로그아웃 단추·예산 배너도 없음). 그동안 읽음 저장 위치를 어디로 할지 확인하고, 화면과 서버가 어긋나면 인계에 적는다. 손님 POST `/articles/read`는 401 JSON이다

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고) + 원래 요청을 다시 보낼 수 없는 수정 동작이 있을 때, 서버 응답 모양(role-guard에서 정한 403·`/unlock`)을 바꿔야 할 때
