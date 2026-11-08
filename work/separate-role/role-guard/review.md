# role-guard 리뷰
결과: 통과
범위 밖 무기록: 0

## 범위 확인
- 검사 명령 재실행: 인계 숫자와 일치. `cargo fmt --check` 0, `clippy --all-targets -D warnings` 0(종료 코드 0), `cargo test` 618 통과 0 실패, `./build.sh --release` 성공, `pnpm lint`·`typecheck` 0, `pnpm test` 1 파일 7 통과, `pnpm test:e2e` 39 통과 5 건너뜀 0 실패. `pnpm format:check`는 미추적 `.playwright-mcp/page-2026-11-08T07-30-59-027Z.yml` 경고 1개로 실패(커밋된 파일 아님, 코드 변경과 무관)
- 범위 밖 판단:
  - 없음. 바뀐 파일(`e2e/auth.spec.ts`·`helpers.ts`, `web.rs`, `web/` 아래 guard·layout·login·settings·status·stocks·timeline와 각 시험, `templates/base.html`·`app_layout.html`)은 모두 수정 가능 범위 안. `work/separate-role/metrics.csv`·`work/metrics.csv`는 run 기록 파일

## Opus 확인
- 브리프에 없던 결정이 인계에 빠짐없이 있는가: 예. 시험 도우미 두 개, `budget_banner_for` 분리와 상태·설정의 `member: true`, `body_attrs` 블록, Edit 403이 메서드와 무관, `Need`·`required` 공개, OPTIONS는 Member, `check_password` 공유가 적혀 있다
- 테스트 기대값을 바꾸거나 빼서 통과시키지 않았는가: 예. Rust 기대값 변경 다섯 곳(`guard_tests` 두 개를 `GUEST_VIEW=off`로 옮김은 브리프 함정에 예고됨, `stocks_tests` 두 개, `timeline_tests` 하나)은 모두 인계에 이름이 있고, 브리프가 정한 응답 규칙(손님 GET `/` 200, 손님 수정 POST 403, 손님 POST `/articles/read` 401 JSON)에서 그대로 따라 나온다. 옛 기대(307)를 지키면 브리프 완료 조건과 모순이다. 옮긴 시험은 같은 확인을 `GUEST_VIEW=off`에서 계속 하므로 뜻이 사라지지 않았다. E2E 변경은 브리프가 허락한 두 시험과 `login()`뿐이고, 손님 `data-access`·로그아웃 없음 확인이 더해졌다. 다른 spec은 바뀌지 않았다
- 제약과 프로젝트 규칙을 지켰는가: 예. 분류는 기본 닫힘(목록 밖 POST `/no/such/post` 403 시험 있음), Origin 검사가 맨 앞, `X-Robots-Tag`는 바깥 `guard`가 `check` 결과에 붙여 Origin 거절 응답에도 붙는다. 실패 로그는 `client_key`만, 비밀번호는 응답·로그에 없다. 비밀번호 창·손님 읽음 JS는 만들지 않았다. 손님 읽음 숨김은 전체 화면과 htmx 조각 모두 적용(조각 분기 앞에서 바꿈)
- 인계가 실제 변경과 맞는가: 예. diff와 인계의 커밋·시험 이름·도우미·다음 작업 영향이 일치

참고(반려 사유 아님):
- Public `/login`은 GET·POST만이라 HEAD `/login`은 Member가 되어 손님에게 `/login?next=%2Flogin`으로 307이 돈다(예전에는 모든 메서드 공개). 브리프 규칙대로이고 브라우저는 HEAD를 쓰지 않아 영향이 없다. 상태 확인 도구가 HEAD `/login`을 쓰게 되면 다시 본다
- `check_password`의 `HeaderValue::from_str(..).expect(..)`는 예전 `if let Ok`보다 엄격하지만 쿠키 값이 base64url·ASCII 속성뿐이라 실패할 길이 없다
- `articles_read_needs_member_only`의 회원 쪽은 401·403이 아님만 본다(본문이 비어 핸들러가 400). 가드 통과 확인으로는 충분하다

## 반영한 것 (통과일 때)
- 영구 문서: DECISIONS.md 1장 "16단계"·"16단계 계획 세부" 줄 근거에 `separate-role/role-guard`를 더하고, 새 줄 "16단계 role-guard 구현 세부"(분류 세부, 응답 모양, `noindex` 위치, `check_password`, 배너·`data-access`, 손님 읽음, 시험 도우미, 기대값 변경 목록; 구현 중 정함, 보고 후 이의 없음). STATE.md "지금 하는 일"을 다시 씀(2개 통과, 단계 중간 배포 안 함 주의). SPEC.md FR-1 16단계 문단과 8장 설정 표는 이미 구현과 같아 고치지 않음
- 다음 브리프 확인: guest-limits 브리프는 인계("`Extension<Access>`", "모든 응답에 `X-Robots-Tag`")와 맞다. "볼 위치"에 guard의 `check`/`guard` 구조와 셀 위치, `Access`를 아직 받지 않는 `stocks::list` 등 조각 핸들러, 새 시험 도우미를 더함. "알려진 함정"에 손님 `/stocks` 3초 폴링 세 곳이 1분 60번에 닿을 수 있음을 더함(닿으면 상위로). unlock-ui "알려진 함정"에 `/unlock` 성공 뒤에도 화면 `data-access`가 `guest`로 남는 점과 손님 `/articles/read` 401을 더함
