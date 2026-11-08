# 지금 상태

> 읽는 때: 세션을 시작할 때 맨 먼저. 지금 하는 일, 답 대기, 다음 할 일만 둔다(화면 한 장 안).
> 이 문서는 덧붙이지 않고 통째로 다시 쓴다(agentflow review가 작업 통과 때, `/handoff`가 단계 밖 작업 뒤). 끝난 일의 자세한 기록은 [DECISIONS.md](DECISIONS.md) 3장, 단계 표·작업 방법은 [DEVELOPMENT.md](DEVELOPMENT.md)에 둔다. 실제와 다르면 `git log`와 `git status`를 먼저 본다.

마지막 갱신: 2026-11-08 21:37 (한국 시간), 16단계 role-guard 리뷰 통과

- **지금 하는 일: 16단계 보기·수정 권한 나누기(`separate-role`, 브랜치 `af/separate-role`)**. 계획은 `work/separate-role/PLAN.md`, 결정은 DECISIONS.md 1장 "16단계" 줄들. 작업 4개 중 2개 통과: ① access-token(세션 토큰에 `sub`·`role`·`edit_exp`, `Access`, 설정 `GUEST_VIEW`·`EDIT_UNLOCK_MINUTES`) ② role-guard(guard 기본 닫힘 분류 Public/Guest/Member/Edit, 손님은 GET `/`·`/stocks*`만, 수정 권한 없으면 403 + `HX-Trigger: unlock-required`, `POST /unlock`, 모든 응답 `X-Robots-Tag: noindex, nofollow`, 손님에게 로그아웃·예산 배너·주인 읽음 숨김, `data-access`; 커밋 `cd8327a`, 리뷰 때 `/check` 통과: 단위 618·E2E 39 통과, 건너뜀 5). 다음은 guest-limits(DB 읽기 `spawn_blocking`, 손님 GET IP별 1분 60번, 손님 타임라인 90개) → unlock-ui(비밀번호 창, 손님 읽음 `localStorage`, 문서). 지금 브랜치 상태에서 손님은 수정 버튼을 눌러도 아무 반응이 없다(비밀번호 창은 unlock-ui 몫이라 단계 중간에는 배포하지 않는다). 배포하면 옛 쿠키는 손님이 되어 한 번 다시 로그인해야 한다. `pnpm format:check`는 미추적 `.playwright-mcp/` 때문에 실패한다(코드와 무관, 지우거나 prettier 무시 목록에 넣을지는 사용자 몫).
- **단계 밖(배포 전)**: 상단 메뉴 현재 위치 굵게(`app.js`, `971147b`)는 main에 커밋만 됐고 본 Docker에는 아직 배포 안 함(전용 테스트 없음).
- **15단계 고정 주소(끝, main `9155cc6`·기록 `f3a081a`, 2026-11-07 17:10 본 Docker 배포)**: 이름 붙은 터널(`TUNNEL_TOKEN`)로 고정 주소 접속, 토큰이 없으면 임시 주소. 시험 터널 확인(연결 4개, 로그인 쿠키 `Secure`, 재시작 뒤 같은 주소, 상태 화면 "연결됨")과 본 배포 확인(고정 주소 `/`가 `307 → /login`, tunnel 연결 4개), 사용자의 휴대폰 접속·로그인 확인까지 끝. 작업 기록은 보관소 `../Ripple-archive/커스텀도메인/`, 자세한 것은 DECISIONS.md 2장·3장. 남은 것: 같은 터널 토큰을 시험 Docker와 본 Docker에서 함께 쓰지 않기, 시험 터널 토큰은 대화에 한 번 남았으니 마음에 걸리면 새로 만들기.
- **최근 끝난 일(2026-11-07)**: ① 노컷뉴스 TLS 회귀 수정(수집용 클라이언트 native-tls) 본 Docker 배포 14:36, 회차 765 success(소스 73개 실패 0), 커밋 `0125dc3` ② v0.3.0 GitHub 릴리스(태그 `v0.3.0`, `06b11d7`). **v0.2.0 이하에서는 업데이트할 수 없음**(사용자 답 B, DEVELOPMENT.md 7장). 기록은 DECISIONS.md 3장 맨 위 두 줄.

- **14단계 Rust 전환(끝: R13 커밋 `bc97679`, 2026-11-07 본 Docker 배포)**: 설계 문서 [RUST_MIGRATION.md](RUST_MIGRATION.md), 결정은 DECISIONS.md 1장 "14단계" 줄들, 기록은 3장. 남은 확인: ① KIS 주가 띠(Rust)를 실제 키로 아직 안 돌렸다(월요일 09:00 첫 갱신 뒤 확인) ② 휴대폰·PC 화면 확인(사용자 몫). rustup 1.29.2·Rust 1.99.1은 2026-10-31 사용자가 설치함.

- **토픽 유지 개선(끝, 커밋 `46e3f58`, 2026-10-30 14:14 배포)**: 첫 정기 토픽 갱신 확인 끝. 15:57:32 시작, Haiku 1회 $0.0078(15:57:44) 성공, 실패 기록 없음, 진행 중 토픽 15개(끝남 136). 로그 줄은 16:00 배포로 컨테이너가 바뀌어 남지 않아 DB(`settings`·`usage_logs`·`topics`)로 확인.
- **단계**: 0~15단계 완료. 본 Docker는 15단계(고정 주소)까지 배포됨(2026-11-07 17:10, 기록은 DECISIONS.md 3장). 12·13단계 휴대폰·PC 화면 확인 결과는 기록이 없다.
- **사용자가 할 일(개발 환경 정리에서 남음)**: ① `sudo chown -R "$(whoami)":staff ~/.npm`(playwright MCP) ② GitHub 토큰을 `GITHUB_PERSONAL_ACCESS_TOKEN`으로 셸 환경에 넣기(github MCP, 이번 세션에서도 연결 실패로 표시됨) ③ 새 세션에서 `/mcp`로 확인.
- **사용자 답 대기: 키워드·채점 점검의 개선안 I1~I9(2026-10-30, DECISIONS.md 2장 "키워드·채점 점검")**: 코드 오류 B1·B2 수정과 개선 후보. I4·I6·I8은 아래 AI 비용 질문과 겹치므로 합쳐 A/B·추천으로 다시 묻는다.
- **사용자 답 대기: AI 비용 줄이기(2026-10-23, 무료 조사만, 구현 전)**: 본 DB 사용량으로 월 약 $2.6 추정. 조사 결과는 DECISIONS.md 2장 "AI 비용 줄이기 조사".
  - 사용자에게 물은 것: 토픽 갱신 모델 교체(품질 비교 유료), 일본어 제목 빼기, Jev 질문 문구 줄이기(유료 비교), 시장·토픽을 기사당 한 번에 묻기, 키워드 재생성 매주, 토픽 수·갱신 간격, 목표 금액. 답을 받으면 DECISIONS.md 1장에 적고 시작
- **13단계(보유 종목 주가 띠): 끝, 커밋 `ddde1a5`, 2026-10-30 16:00 본 Docker 배포**: 결정은 DECISIONS.md 1장 "13단계"·"13단계 세부"·"13단계 구현 세부" 줄. 한 일: `src/server/prices/`(KIS 클라이언트·장 시간·1분마다 받기·저장/띠 읽기), Finnhub `quote`, 표 `stock_prices`(마이그레이션 0010), 스케줄러 `refreshPrices`, 타임라인 띠 `price-strip.tsx`, 상태 화면 "주가" 줄, 환경 변수 `KIS_APP_KEY`·`KIS_APP_SECRET`·`KIS_ENV`(compose·`.env.example`), SPEC(FR-11 등)·README. 12단계 읽음 E2E 간헐 실패도 시험 쪽에서 고침(DECISIONS.md 1장). 마지막 `/check`: 서식·린트·타입·단위 446개·빌드(web·worker)·E2E 39개 통과(건너뜀 5개는 모바일). 화면 캡처(데스크톱·휴대폰) 직접 확인.
  - **KIS 실제 확인 끝(2026-10-30 15:49~15:55, 시험용 Docker, 무료, DECISIONS.md 2장 "13단계 KIS 실제 확인")**: 사용자가 모의투자 키를 `.env`에 넣음. 모의 서버의 통합(`UN`) 현재가, 토큰 하루 유효, worker 1분마다 저장, 띠·상태 화면 모두 확인. 시험용 Docker는 내리고 이미지도 지움.
  - **남은 확인**: ① 모의 서버 값이 실전과 같은지(사용자가 증권 앱과 대조, 15:49 `UN` 현대건설 83,500·두산에너빌리티 54,600) ② KIS 미국 시세가 실시간인지(미국 장중 22:30 이후 KIS `last`와 Finnhub `c` 비교, 실시간이면 미국도 KIS로 바꿀지 묻기, 결정 ⑤). 본 Docker 배포 뒤 1분마다 쌓이는 것은 확인함. 같은 키를 두 곳(본·시험 Docker)에서 쓰면 토큰 발급이 1분에 1번이라 서로 막는다(시험용은 `/real-check`대로 KIS 키를 비울 것).
  - 이력 보관: 지금은 무기한(한국 종목 하나에 하루 약 725줄). 줄일지 사용자에게 아직 묻지 않음.
- **(14단계 뒤로 보류) 다음이던 것**: 13단계 남은 확인(위 ①②). 그 뒤 위 답 대기 두 건(키워드·채점 개선안, AI 비용 줄이기)과 13단계 이력 보관 기간을 사용자에게 묻는다. 키워드·채점 점검의 B1(키워드 생성 실패 3건, 사용량 0토큰)은 "숨은 추론 토큰이 max_tokens를 먹는다"는 발견도 원인 후보로 본다(DECISIONS.md 2장). 배포 기록 문서는 커밋 전(요청하면 "배포 기록" 커밋).
- 지난 진행 기록(단계별 체크리스트·배포 기록)은 DECISIONS.md 3장.
