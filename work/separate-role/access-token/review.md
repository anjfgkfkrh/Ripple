# access-token 리뷰
결과: 통과
범위 밖 무기록: 0

## 범위 확인
- 검사 명령 재실행: 인계 숫자와 일치. `cargo fmt --check` 0, `clippy -D warnings` 0, `cargo test` 607 통과 0 실패(session 12, `web::config` 시험 중 `reads_guest_view`·`reads_edit_unlock_minutes` 통과), `./build.sh --release` 성공, `pnpm lint`·`typecheck` 0, `pnpm test` 7 통과, `pnpm test:e2e` 39 통과 5 건너뜀. `pnpm format:check`는 미추적 `.playwright-mcp/page-2026-11-08T07-30-59-027Z.yml` 경고 1개로 실패(커밋된 파일 아님, 코드 변경과 무관)
- 범위 밖 판단:
  - 없음. 바뀐 파일(`.env.example`, `docker-compose.yml`, `server/auth/session.rs`·`session_tests.rs`, `web.rs`, `web/config.rs`·`config_tests.rs`·`login.rs`·`state.rs`)은 모두 수정 가능 범위 안. `work/separate-role/metrics.csv`·`work/metrics.csv`는 run 기록 파일

## Opus 확인
- 브리프에 없던 결정이 인계에 빠짐없이 있는가: 예. `Access` 위치(session.rs), 체크포인트 두 개를 한 커밋으로 묶음(시그니처 변경으로 컴파일 불가), 시험 도우미 기본값, 오류 문구에 값 없음, `i64` 파싱이 적혀 있다
- 테스트 기대값을 바꾸거나 빼서 통과시키지 않았는가: 예. 기존 session 시험은 호출에 `, 30`만 더했다. `payload_swap_is_invalid`의 위조 payload에 새 칸을 더한 것은 제약의 예외(payload 모양을 직접 보는 시험)에 해당하고 기대(무효)는 그대로이며 인계에 이름이 있다. 새 칸이 없으면 서명 검사와 상관없이 손님이 되어 시험이 아무것도 확인하지 않게 되므로 맞는 수정이다
- 제약과 프로젝트 규칙을 지켰는가: 예. 서명 형식 그대로, `WebConfig` Debug는 새 칸을 넣지 않음(비밀 아님, 기존처럼 non_exhaustive), 설정 오류 문구에 값 없음, `div_euclid(1000)` 사용, `edit_exp*1000`은 `checked_mul`. `can_edit`의 `now < edit_exp*1000`은 `exp`의 "초 내림 ≥ exp면 만료"와 같은 경계다. guard·화면 동작은 바뀌지 않음(`is_valid_session_token` = `read_access(..).is_member()`, 기존 E2E 통과)
- 인계가 실제 변경과 맞는가: 예. diff와 인계의 새 시험 이름·변경 파일·다음 작업 영향이 일치

참고(반려 사유 아님): `reads_edit_unlock_minutes`의 "값이 메시지에 없음" 확인은 `": {bad}"`·`"{bad}입"` 꼴만 본다. 지금 문구가 고정 문자열이라 문제는 없다.

## 반영한 것 (통과일 때)
- 영구 문서: DECISIONS.md 1장 "16단계"·"16단계 계획 세부" 줄에 `근거: separate-role/access-token`, 새 줄 "16단계 access-token 구현 세부"(구현 중 정함, 보고 후 이의 없음). STATE.md "지금 하는 일"을 16단계 진행 상태로 다시 씀. SPEC.md 8장 `GUEST_VIEW`·`EDIT_UNLOCK_MINUTES` 줄은 이미 구현과 같아 고치지 않음
- 다음 브리프 확인: role-guard 브리프는 인계("`Access`는 guard에 연결 전", "`WebState.guest_view` 미사용")와 맞다. "알려진 함정"에 `start_web`의 박힌 `guest_view`(off 시험엔 도우미 옵션 필요), 수정 권한 지난 회원 만드는 법(`clock` 넘기기), `.playwright-mcp/` 서식 검사 실패를 더함. guest-limits·unlock-ui 브리프에도 `.playwright-mcp/` 한 줄을 더함
