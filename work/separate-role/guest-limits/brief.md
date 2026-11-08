# guest-limits: 손님 공개에 대한 가용성·노출 대책
모델: Sonnet / effort: medium | 선행: role-guard 통과

## 목표
손님 요청이 몰려도 web이 멈추지 않게 한다. web DB 읽기는 `spawn_blocking`으로 돌리고, 손님 GET은 IP별로 1분에 60번까지만 받는다. 손님 타임라인은 최대 90개로 막고, 손님에게는 키워드 실패의 자세한 이유를 숨긴다.

## 완료 조건
- 요청 제한 단위 시험 통과:
  - 같은 키로 60번은 통과하고 61번째는 429 + `Retry-After`
  - 1분 창이 지나면 다시 통과
  - 다른 키는 따로 센다
  - 키가 10,000개로 차면 지난 창부터 지우고, 그래도 넘치면 새 키는 429
- `guard_tests.rs` 새 시험 통과:
  - 손님 GET `/`를 61번 보내면 61번째가 429
  - 회원은 61번째도 200
  - `/static/*`는 세지 않음
- `read_tests.rs` 새 시험 통과: 손님 최대 90이면 `limit=600`이 90이 되고, 회원은 지금처럼 600까지
- `web/timeline` 시험 통과: 손님 90개에서는 "더 보기" 링크가 없음
- `web/stocks` 시험 통과: 키워드 실패 이유는 손님에게 앞 문장만, 회원에게는 지금처럼 이유까지 보임
- `/check` 전체 통과

## 수정 가능 범위
- `rust/ripple/src/web/`
- `rust/ripple/src/web.rs`
- `rust/ripple/src/server/auth/`
- `rust/ripple/src/server/db/web.rs`
- `rust/ripple/src/server/db/web_tests.rs`
- `rust/ripple/src/server/timeline/read.rs`
- `rust/ripple/src/server/timeline/read_tests.rs`
- `rust/ripple/src/test_support.rs`
- `work/separate-role/guest-limits/`

## 볼 위치
- `rust/ripple/src/server/db/web.rs`의 `WebDb::with`
- `rust/ripple/src/web/`에서 `state.db.with(`를 부르는 아홉 곳(layout·settings·status·stocks·timeline)
- `rust/ripple/src/server/auth/rate_limit.rs`의 `LoginRateLimiter`(모양을 따름)
- `rust/ripple/src/server/timeline/read.rs`의 `parse_timeline_params`·`TIMELINE_MAX`
- `rust/ripple/src/web/stocks.rs`의 `keywords_view`(`failed_text`)
- `docs/DECISIONS.md` 1장 "16단계 공개 보안 대책"·"16단계 보안 대책 계획 세부"

## 미리 정한 판단
- DB 읽기: `WebDb`(또는 `WebState`)에 async 함수 하나를 두고, `tokio::task::spawn_blocking` 안에서 기존 `with`를 부른다. 아홉 곳을 모두 이 함수로 바꾼다. 클로저에 넘기는 값은 소유값(`'static`)으로 옮긴다. 실행기(`new_current_thread`)는 바꾸지 않는다.
  이유: DB를 읽는 동안에도 다른 요청(429 응답, 정적 파일, 로그인)이 막히지 않게 한다. 버린 대안: multi_thread 실행기(DB 잠금이 하나라 이득이 작음), 읽기 연결 여러 개(복잡함)
- 손님 요청 제한기: `GuestRateLimiter`를 `WebState`에 둔다. 1분 고정 창으로 키(`client_key`)마다 횟수를 센다. 기록은 메모리에 두고 키는 최대 10,000개다. 넘치면 창이 지난 키를 지우고, 그래도 넘치면 새 키를 거절한다. 시각은 `state.now()`를 쓴다(시험에서 바꿀 수 있게).
  이유: 결정 ①, 세부 ①. 버린 대안: 미끄러지는 창(기록이 커짐)
- 적용 위치: guard에서 `Access`가 손님이고 분류가 Guest(손님 GET)일 때만 센다. Public(`/static/*`·`/favicon.ico`·`/login`·`/unlock`)과 회원은 세지 않는다. 넘으면 429와 `Retry-After: <남은 초>`, 글자 "요청이 너무 많습니다. 잠시 뒤 다시 시도하세요."를 보낸다. 응답에는 지금처럼 `X-Robots-Tag`가 붙어야 한다.
  이유: 로그인·비밀번호 창은 따로 실패 제한이 있고, 주인이 막히면 안 된다.
- 손님 최대 개수: `parse_timeline_params`에 최대값 인자를 더한다(회원 `TIMELINE_MAX` 600, 손님 새 상수 `GUEST_TIMELINE_MAX` 90). 손님에게는 90에 닿으면 "더 보기"를 그리지 않는다.
- 키워드 실패 글자: 손님이면 `failed_text`를 앞 문장(`lead`)만으로 만든다. 회원이면 지금 그대로 둔다.

## 체크포인트 (각각 검사 통과 후 커밋)
1. DB 읽기 `spawn_blocking`(아홉 곳) → 기존 Rust 시험 모두 통과
2. `GuestRateLimiter`와 guard 적용 → 제한기 단위 시험과 `guard_tests` 새 시험 통과
3. 손님 최대 90개, 키워드 실패 이유 숨김 → `read_tests`·timeline·stocks 새 시험과 `/check` 전체 통과

## 제약
- 기존 시험의 기대값은 바꾸지 않는다. `parse_timeline_params` 호출 모양이 바뀌는 것은 시험 코드 수정으로 보고 기대값은 그대로 둔다
- E2E는 손님으로 1분에 60번을 넘기지 않는다. 넘기는 시험이 생기면 상위로 올린다

## 알려진 함정
- E2E 시험들은 `asClient`로 시험마다 다른 `cf-connecting-ip`를 쓴다. 로그인 전 손님 GET도 세므로, 한 시험 안에서 손님 GET이 60번을 넘지 않는지 확인한다
- `spawn_blocking` 안에서 잠금이 깨지면(panic) 지금처럼 오류 글자로 돌려준다. `JoinError`도 같은 오류 글자로 바꾼다

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고) + 회원 화면의 동작이 바뀌어야 할 때
