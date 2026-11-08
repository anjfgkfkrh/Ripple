# guest-limits 질문
> 사실만 묻는다. 판단은 묻지 않는다. 하위는 "답"에 사실만 쓴다.

## 질문 (리뷰)
### rust/ripple/src/server/auth.rs
(브리프 수정 가능 범위는 `rust/ripple/src/server/auth/` 폴더이고 이 파일은 그 밖이다. 인계에는 "범위 밖 변경 없음"으로 적혀 있다. 바뀐 내용은 `pub mod guest_limit;` 한 줄이다.)
1. 언제 바꿨나? (어느 커밋, 어느 체크포인트)
2. 무엇 때문에 바꿨나?
3. 바꾸지 않으면 무엇이 실패하나?

## 답 (하위)
### rust/ripple/src/server/auth.rs
1. 커밋 3d90760 "guest-limits: 손님 GET 요청 제한", 체크포인트 2. `pub mod guest_limit;` 한 줄을 더했다.
2. 새 파일 `server/auth/guest_limit.rs`(`GuestRateLimiter`)를 모듈로 등록하려면 부모 모듈 선언 파일 `server/auth.rs`에 `mod` 선언이 필요하다. 인계에 "범위 밖 변경 없음"이라 적은 것은 이 파일을 범위 밖으로 따로 세지 않은 기록 누락이다.
3. 바꾸지 않으면 `guest_limit.rs`가 컴파일 대상에 들어가지 않아 `crate::server::auth::guest_limit::GuestRateLimiter`를 찾지 못해 `web/state.rs`와 `web/guard.rs`가 빌드 오류를 낸다.
