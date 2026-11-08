# unlock-ui 리뷰
결과: 통과
범위 밖 무기록: 0

## 범위 확인
- 검사 명령 재실행: 인계 숫자와 일치. `cargo fmt --check` 0, `clippy -D warnings` 0, `cargo test` 629 통과 0 실패, `./build.sh --release` 성공, `pnpm lint`·`typecheck` 0, `pnpm test` 1 파일 7 통과, `pnpm test:e2e` 49 통과 5 건너뜀(모바일) 0 실패. `pnpm format:check`는 미추적 `.playwright-mcp/page-2026-11-08T07-30-59-027Z.yml` 1개로 실패(커밋된 파일 아님, 이 작업과 무관)
- 범위 밖 판단:
  - 없음. 바뀐 파일은 `README.md`·`release/INSTALL.md`·`e2e/guest.spec.ts`·`rust/ripple/static/app.js`·`rust/ripple/templates/app_layout.html`과 기록 파일(`PLAN.md`, 작업 폴더)뿐
  - `work/separate-role/metrics.csv`·`work/metrics.csv`·`.playwright-mcp/`: run·도구 기록 파일, 작업 변경 아님

## Opus 확인
- 브리프에 없던 결정이 인계에 빠짐없이 있는가: 예. 창 제목 "비밀번호 확인", 단추 부품(`primary`·`dialog_cancel`), `data-idle-label`·`unlockButton`, 다시 보낸 뒤 403 글자 자리(`data-unlock-failed`)와 지우는 때, 폼 아닌 요소는 `click`, 시험용 데이터를 `guest.spec.ts` 안에 넣은 것, `.env.example` 그대로, Cloudflare 출처 문서, README 읽음 문구가 모두 diff와 같다. 창 마크업은 `ui/` 매크로가 없어 `stocks/delete_dialog.html`의 클래스를 그대로 옮겼다(같은 모양, 확인함)
- 테스트 기대값을 바꾸거나 빼서 통과시키지 않았는가: 예. 기존 시험 파일은 고치지 않았고 `guest.spec.ts`만 새로 더했다(5개 × 2프로젝트). 완료 조건의 다섯 항목(손님 보기·로그아웃 없음, 설정 → 로그인, 비밀번호 창 오류·맞으면 같은 검색 실행·입력 값 유지, 상태·설정 열림, 손님 읽음 링크·버튼·새로 고침·`article_reads` 0·주인 문맥 안 읽음)을 모두 본다
- 제약과 프로젝트 규칙을 지켰는가: 예. 외부 스크립트 없음, `docs/` 안 고침. 비밀번호 검사·권한 판단은 서버(`/unlock` 204/401, 403 + `HX-Trigger`)만 하고 JS는 창과 다시 보내기만 한다(결정 ⑤·⑩). 3초 폴링 건너뛰기는 `dialog[open]`이라 비밀번호 창이 열려 있어도 같이 멈춘다. htmx 2는 `htmx:responseError`를 `htmx:afterRequest`보다 먼저 내므로 `retryElt` 판단 순서가 맞다. Cloudflare 메뉴 이름(Security rules, Create rule > Rate limiting rules, Rule name, With the same characteristics, When rate exceeds, Then take action/Choose action, Duration, Deploy)은 공식 문서 `create-zone-dashboard`와 직접 대조해 같다
- 인계가 실제 변경과 맞는가: 예. 커밋 두 개(체크포인트 1·2 묶음, 3)와 인계가 같다. 화면 캡처 6장 확인: 휴대폰 비밀번호 창 오류("비밀번호가 맞지 않습니다.")가 창 안에 보이고, 데스크톱 손님 타임라인은 상단 메뉴 넷, 로그아웃 없음

참고(반려 사유 아님):
- "다시 보낸 요청도 403이면 글자를 보인다" 경로는 E2E가 없다(완료 조건에 없음). 코드로만 확인
- 손님 타임라인 캡처는 시험 DB에 기준 이상 기사가 없어 빈 목록 화면이다. 카드가 있는 손님 화면은 읽음 시험이 거치지만 캡처는 없다
- 손님이 `/unlock`에 성공한 직후 화면이 새로 고칠 때까지 손님 모양이고 읽음이 `localStorage`에 쌓이는 문제(인계 "발견한 것")는 화면 동작이라 STATE.md "사용자에게 물을 것"으로 올렸다(A 그대로(추천) / B 새로 고침)
- 비밀번호 창이 열린 채 원래 폼이 다른 조각으로 바뀌면(`isConnected` 거짓) 아무 일도 하지 않는다. 폴링이 창이 열린 동안 멈추므로 지금 화면에서는 생기지 않는다

## 반영한 것 (통과일 때)
- 영구 문서: DECISIONS.md 1장 "16단계: 보기·수정 권한 나누기"·"16단계 계획 세부"·"16단계 공개 보안 대책"·"16단계 보안 대책 계획 세부" 줄에 `separate-role/unlock-ui` 근거를 더하고, 새 줄 "16단계 unlock-ui 구현 세부"(창 위치·흐름·다시 보내기·단추 풀기·손님 읽음·권한 얻은 직후 화면·README/INSTALL·E2E; 구현 중 정함, 보고 후 이의 없음). STATE.md "지금 하는 일"을 4개 모두 통과·사람 확인 대기로 다시 쓰고, "사용자에게 물을 것"에 권한 얻은 직후 화면(A/B), 폴링 답 뒤 처리 문구를 고침. SPEC.md는 FR-1 16단계 문단·환경 변수 표·10장 16단계가 이미 구현과 같아 고치지 않음
- 다음 브리프 확인: 이 단계의 마지막 작업이라 다음 브리프 없음. `대기`인 다른 작업도 없음
