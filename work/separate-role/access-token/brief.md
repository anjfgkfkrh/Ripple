# access-token: 세션 토큰에 사용자·역할·수정 권한 시각 넣기
모델: Sonnet / effort: medium | 선행: 없음

## 목표
세션 토큰 payload에 `sub`·`role`·`edit_exp`를 더하고, 토큰을 읽어 `Access`(손님 / 회원)를 돌려주는 함수를 만든다. web 설정에 `GUEST_VIEW`·`EDIT_UNLOCK_MINUTES`를 더한다. 화면·guard 동작은 이 작업에서 바꾸지 않는다.

## 완료 조건
- `session_tests.rs` 새 시험 통과: 새 토큰을 읽으면 회원(`sub`·`role` = `"owner"`)이고 `edit_exp` 전에는 수정 가능, `edit_exp` 이후에는 수정 불가지만 회원은 유지, `exp`가 지나면 손님, `pwd` 지문이 다르면 손님, `sub`·`edit_exp`가 없는 옛 토큰이면 손님, 서명이 틀리면 손님
- `config_tests.rs` 새 시험 통과: `GUEST_VIEW` 비었음·`on` → 켬, `off` → 끔, 그 밖의 값 → 설정 오류. `EDIT_UNLOCK_MINUTES` 비었음 → 30, `1`·`120` 받음, `0`·`121`·`abc` → 설정 오류. 오류 메시지에 값이 들어가지 않음
- `/check` 전체 통과(기존 시험 포함)

## 수정 가능 범위
- `rust/ripple/src/server/auth/`
- `rust/ripple/src/web/`
- `rust/ripple/src/web.rs`
- `rust/ripple/src/test_support.rs`
- `docker-compose.yml`
- `.env.example`
- `work/separate-role/access-token/`

## 볼 위치
- `rust/ripple/src/server/auth/session.rs`의 `create_session_token`·`is_valid_session_token`
- `rust/ripple/src/web/config.rs`의 `read_web_config`, `rust/ripple/src/server/env.rs`의 `read_port`(설정 오류 모양)
- `rust/ripple/src/web.rs`의 시험 도우미 `start_web`(새 설정 값의 기본을 넣는 곳)
- `src/web/`에서 바꾸는 것은 설정·상태·로그인 토큰 발급뿐이다. guard 동작과 화면은 바꾸지 않는다
- `docs/DECISIONS.md` 1장 "16단계" 두 줄
- `docs/SPEC.md` 8장 `GUEST_VIEW`·`EDIT_UNLOCK_MINUTES` 줄

## 미리 정한 판단
- `Access`는 `enum Access { Guest, Member { sub: String, role: String, edit_until: EpochMs } }`이고, 메서드는 `is_member()`와 `can_edit(now)`이다. 놓을 곳은 `server/auth/session.rs` 또는 새 `server/auth/access.rs`(작은 결정, 인계에 적기).
  이유: 계정 기능 때 `sub`·`role`이 오는 곳만 바꾸면 되게 한다. 버린 대안: bool 두 개(역할을 더할 자리가 없음)
- 토큰 발급은 `create_session_token(config, now, edit_minutes)`처럼 수정 분을 받는다. payload는 `{"sub":"owner","role":"owner","pwd":…,"iat":…,"exp":iat+30일,"edit_exp":iat+분*60}`이다. 서명 형식(`base64url(payload).base64url(HMAC)`)은 그대로 둔다.
  이유: 쿠키 하나로 두 겹(로그인 30일·수정 N분)을 표현하고, DB를 쓰지 않는다(D-36). 버린 대안: 쿠키 둘(만료 처리가 두 번이 됨)
- 읽기는 `read_access(token, config, now) -> Access`로 바꾼다. 기존 `is_valid_session_token`은 `read_access(..).is_member()`로 남기거나 호출하는 곳을 바꾼다. `has_valid_session`의 뜻(회원인가)은 바꾸지 않는다.
  이유: guard 동작은 다음 작업(role-guard)에서 바꾼다. 이 작업 뒤에도 기존 E2E가 그대로 통과해야 한다.
- 새 칸(`sub`·`role`·`edit_exp`)이 하나라도 없거나 타입이 틀리면 손님이다.
  이유: 옛 쿠키를 받아 주면 수정 권한 판단이 애매해진다(배포 뒤 한 번 다시 로그인, 결정됨).
- `WebConfig`에 `guest_view: bool`, `edit_unlock_minutes: i64`를 더하고 `WebState`에 그대로 싣는다. 로그인 성공 때 쓰는 분은 이 값이다.
- compose web 환경에 `GUEST_VIEW: ${GUEST_VIEW:-}`, `EDIT_UNLOCK_MINUTES: ${EDIT_UNLOCK_MINUTES:-}`를 더하고, `.env.example`에는 설명 주석과 빈 값 줄을 더한다.

## 체크포인트 (각각 검사 통과 후 커밋)
1. `server/auth/` 토큰 payload·`Access`·`read_access` → `session_tests` 새 시험 통과
2. `web/config.rs`·`state.rs`·`login.rs`(새 분 넘기기)·compose·`.env.example` → `config_tests` 새 시험과 `/check` 전체 통과

## 제약
- 기존 시험의 기대값은 바꾸지 않는다. 예외: 토큰 payload 모양을 직접 보는 `session_tests`의 시험은 새 칸에 맞춰 고쳐도 된다(인계에 이름을 적는다)
- 비밀 값(`APP_PASSWORD`·`SESSION_SECRET`)을 로그·오류·Debug 출력에 넣지 않는다
- `.env`는 읽지 않는다

## 알려진 함정
- `now`는 ms(`EpochMs`)이고 토큰의 시각은 초다. 기존 코드처럼 `div_euclid(1000)`을 쓴다

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고) + 기존 E2E가 이 작업만으로 깨지는 경우
