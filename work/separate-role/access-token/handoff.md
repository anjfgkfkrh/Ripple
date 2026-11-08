# access-token 인계
커밋: 06d5685 | 상태: 완료

## 검사 결과 (실행한 명령과 출력 숫자)
| 명령 | 출력 숫자 |
|---|---|
| cargo fmt --check | 오류 0 |
| cargo clippy --all-targets -- -D warnings | 경고 0 |
| cargo test | 607 통과, 0 실패 (session 12, config 27 포함) |
| ./build.sh --release | 오류 0 |
| pnpm format:check | 경고 1 (미추적 `.playwright-mcp/page-*.yml`, 이번 변경과 무관) |
| pnpm lint / typecheck | 0 / 0 |
| pnpm test | 7 통과 |
| pnpm test:e2e | 39 통과, 0 실패, 5 건너뜀(mobile) |

## 브리프에 없던 결정
- `Access`는 `server/auth/session.rs`에 둠(작은 결정).
- 체크포인트 1·2는 `create_session_token` 시그니처가 바뀌어 web 쪽 없이는 컴파일되지 않아 한 커밋으로 묶음.
- 기존 `session_tests`의 `create_session_token(.., now)` 호출에 `, 30`을 더함(기대값 변경 없음). `payload_swap_is_invalid`는 payload에 새 칸을 더함(옛 칸만이면 새 규칙으로도 손님이라 시험 의미가 사라지므로).
- `web.rs` 시험 도우미: `cookie()`가 `state.edit_unlock_minutes`를 넘기고, `start_web`의 WebConfig에 `guest_view: true`, `edit_unlock_minutes: 30`을 넣음.
- 설정 오류 메시지는 변수 이름만 넣고 값은 넣지 않음. `EDIT_UNLOCK_MINUTES`는 `parse::<i64>()`로 읽음(소수·`1e1` 등은 오류).
- 새 시험: session_tests 4개(`new_token_is_member_with_edit_window`, `expired_or_wrong_fingerprint_is_guest`, `old_token_without_new_fields_is_guest`, `bad_signature_is_guest`), config_tests 2개(`reads_guest_view`, `reads_edit_unlock_minutes`).

## 발견한 것
- `pnpm format:check`가 미추적 `.playwright-mcp/`에서 실패한다.

## 상위로 올린 것
없음

## 다음 작업에 영향
- `WebState.guest_view`는 아직 아무 곳에서도 쓰지 않는다. `Access`·`read_access`는 guard에 연결되지 않았다(`has_valid_session`은 회원 여부 그대로).
- 배포 뒤 옛 쿠키는 손님이 되어 한 번 다시 로그인해야 한다.
