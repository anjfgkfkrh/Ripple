# guest-limits 리뷰
결과: 통과
범위 밖 무기록: 1

## 범위 확인
- 검사 명령 재실행: 인계 숫자와 일치. `cargo fmt --check` 0, `clippy --all-targets -D warnings` 종료 0, `cargo test` 629 통과 0 실패, `./build.sh --release` 성공, `pnpm lint`·`typecheck` 0, `pnpm test` 1 파일 7 통과, `pnpm test:e2e` 39 통과 5 건너뜀(모바일) 0 실패. `pnpm format:check`는 미추적 `.playwright-mcp/page-2026-11-08T07-30-59-027Z.yml` 1개로 실패(커밋된 파일 아님, 코드 변경과 무관)
- 범위 밖 판단:
  - `rust/ripple/src/server/auth.rs`: 브리프 빈틈 — 범위가 `server/auth/` 폴더였는데 새 모듈 `guest_limit.rs`를 넣으려면 부모 모듈 파일에 `pub mod guest_limit;` 한 줄이 필요하다(없으면 `web/state.rs`·`guard.rs` 빌드 오류). 인계에는 "범위 밖 변경 없음"으로 빠져 있었고 `questions.md` 답으로 확인. 다음 브리프에서 폴더 범위에 새 모듈을 둘 때는 부모 `mod` 파일도 범위에 넣는다
  - `work/separate-role/metrics.csv`·`work/metrics.csv`·`.playwright-mcp/`: run·도구 기록 파일, 작업 변경 아님

## Opus 확인
- 브리프에 없던 결정이 인계에 빠짐없이 있는가: 예(위 `server/auth.rs` 한 줄 기록 누락 말고는). `Arc<WebDb>`+`read`, async가 된 함수들, 바꾼 곳 열 곳(브리프 "아홉 곳"보다 하나 많음, diff로 확인), `JoinError` 문구, 제한기 파일 위치와 가득 찼을 때 `Retry-After`, `parse_timeline_params` 4번째 인자·`build_body(guest)`, 회원 "더 보기" 그대로, `failed_lead`·`hide_failure_reasons`·조각 핸들러 `Extension<Access>`가 모두 적혀 있다
- 테스트 기대값을 바꾸거나 빼서 통과시키지 않았는가: 예. 기존 시험 변경은 `read_tests`의 `parse_timeline_params` 호출에 `TIMELINE_MAX` 인자를 더한 것뿐이고 기대값은 그대로다(브리프 제약이 허락한 모양 변경). 새 시험: 제한기 4개(60/61·`Retry-After` 50, 창 경계 `WINDOW_MS-1` 거절·`WINDOW_MS` 통과, 키별, 가득 참 → 지난 창 지움·새 키 거절·기존 키 계속), `guard_tests` 3개(61번째 429 + `Retry-After: 60` + `X-Robots-Tag` + 글자, 1분 뒤 통과, 회원 61번 200, 정적·로그인 70번씩 뒤 `/` 200), `read_tests` 손님 90·회원 600, `timeline_tests` 손님 90개·"더 보기" 없음·60에서는 `?limit=90` 링크·회원 100개, `stocks_tests` `/stocks`·`/stocks/list`에서 손님 앞 문장만·회원 이유까지, `web_tests` `read` 결과·panic 문구·깨진 잠금 문구
- 제약과 프로젝트 규칙을 지켰는가: 예. 실행기는 그대로(current_thread), 세는 곳은 guard `check`의 `Need::Guest && !access.is_member()`(Public·회원 제외), 429에도 바깥 `guard`가 `X-Robots-Tag`를 붙인다(시험 있음). 시각은 `state.now()`. 손님 `/stocks` 폴링이 상한에 닿는 문제는 브리프 함정대로 세는 규칙을 바꾸지 않고 상위로 올렸다
- 인계가 실제 변경과 맞는가: 예. 커밋 세 개가 체크포인트 셋과 맞고, 인계의 함수·시험·문구가 diff와 같다

참고(반려 사유 아님):
- 손님 `/stocks` 폴링 429(인계 "상위로 올린 것"): 확인함. 세 조각은 각자 그 종류에 만드는 중(대기·생성 중) 항목이 있을 때만 3초마다 폴링한다(`stocks.rs` `poll`). 06:00 자동 재생성(`schedule/plan.rs` `keyword_plan`)은 자동 토픽 말고 모든 항목을 한꺼번에 대기열에 넣으므로 그동안 셋이 함께 1분 60번 + 화면 1번이 된다. 결정 ①의 숫자·규칙이나 화면 동작을 바꿔야 풀리므로 사람에게 올렸다(STATE.md "사용자 답 대기", 선택지 A 손님 폴링 늦추기(추천)·B 조각 세지 않기·C 한도 올리기)
- `server/status.rs:34` `get_system_status`가 아직 동기 `db.with`다. 결정 ①의 "web DB 읽기 `spawn_blocking`"에서 빠진 하나지만 회원 전용 `/status/system`의 가벼운 읽기이고 브리프 범위 밖 파일이라 남은 일로 기록만 했다
- 키워드 실패 숨김 시험은 종목 키워드와 `/stocks`·`/stocks/list`만 본다. 시장·주제 렌즈 키워드와 `/stocks/markets`·`/stocks/topics/list`도 같은 `reload_for` → `hide_failure_reasons`를 타는 것을 코드로 확인했지만 시험은 없다
- 손님 제한은 HEAD `/`·`/stocks*`도 센다(분류 Guest). 브라우저 동작에는 영향 없음

## 반영한 것 (통과일 때)
- 영구 문서: DECISIONS.md 1장 "16단계 공개 보안 대책"·"16단계 보안 대책 계획 세부" 줄에 `근거: separate-role/guest-limits`, 새 줄 "16단계 guest-limits 구현 세부"(`read`·async 함수·panic 문구, 남은 동기 읽기, 제한기 위치·세는 곳·가득 참 `Retry-After`, 90개·"더 보기", `failed_lead`·조각 `Access`, `server/auth.rs` 한 줄; 구현 중 정함, 보고 후 이의 없음). STATE.md "지금 하는 일"을 다시 쓰고(3개 통과), "사용자 답 대기"에 손님 폴링 429(A/B/C, 추천 A)와 "남은 작은 일"에 `/status/system` 동기 읽기를 둠. SPEC.md FR-1 16단계 문단은 이미 구현과 같아 고치지 않음
- 다음 브리프 확인: unlock-ui 브리프는 인계("손님 GET E2E가 한 시험에서 60번을 넘으면 429")와 맞다(제약 "E2E는 손님으로 1분 60번을 넘기지 않는다"는 guest-limits 브리프에 있었고 unlock-ui에는 없어서 함정으로 더함). "알려진 함정"에 손님 GET 1분 60번·폴링 조각이 더 센다·세는 규칙은 사용자 답 대기·손님 90개 "더 보기" 없음을 한 줄로 더함
