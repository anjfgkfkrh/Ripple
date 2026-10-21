# 개발 현황과 이어서 하기

새 세션은 이 문서부터 읽는다. CLAUDE.md는 자동으로 읽히고, 요구사항·결정 기록·미결정 목록은 [SPEC.md](../SPEC.md)에 있다(결정은 12장, 실측 결과는 11장). 사용법은 [README.md](../README.md).

이 문서는 개발하는 동안 계속 고친다. 아래 "지금 상태"가 실제와 다르면 git log와 `git status`를 먼저 본다.

## 1. 지금 상태

마지막 갱신: 2026-10-21 18:20 (한국 시간)

- **진행 중: 9단계** — Cloudflare Tunnel, 실행 안내 README, D-27~D-29. 질문은 모두 답을 받았고 SPEC 12장에 적었다.
- 9단계에서 할 일
  - [x] compose에 `tunnel` 서비스(cloudflared `2026.10.0`, Quick Tunnel, `--metrics 0.0.0.0:20241`)
  - [x] 상태 화면에 지금 임시 주소와 마지막 백업 표시(`src/server/tunnel.ts`, `TUNNEL_METRICS_URL`)
  - [x] worker 자동 백업: 매일 04:00(한국 시간), 호스트 `./backups`, 최근 7개, 놓쳤으면 켜질 때 한 번(`src/server/backup.ts`, 스케줄러, `BACKUP_DIR`)
  - [x] 시험용 Docker를 본 Docker와 따로 띄울 수 있게 compose 포트·백업 폴더를 변수로(`WEB_PORT`, `BACKUP_HOST_DIR`)
  - [x] README를 실행 안내 중심으로 다시 쓰기
  - [x] 테스트 추가와 전체 검사: 단위 329, E2E 30 통과(4개는 설계상 건너뜀), lint·타입·포맷·빌드 통과
  - [x] E2E 로그인 간헐 실패 원인 찾고 고침(아래 4장)
  - [x] 실제 확인: 시험용 Docker로 임시 주소 접속·로그인·서버 액션, 백업·복원 확인 후 내림(SPEC 11장)
  - [ ] 완료 보고(완료 기준: 휴대폰에서 임시 URL로 접속·로그인 — 실제 휴대폰은 사용자가 확인)
- 본 Docker(사용자 것)는 아직 8단계 이미지로 돌고 있다. 9단계를 쓰려면 `docker compose up -d --build`(사용자가 하거나 요청할 때). 그러면 터널이 켜져 공개 임시 주소가 생긴다.
- 사용자 답을 기다리는 질문: 없음
- 다음 단계: 9단계가 마지막이다(SPEC 10장). 끝나면 MVP 완료 보고.

## 2. 단계별 현황

| 단계 | 내용 | 상태 | 커밋 |
|---|---|---|---|
| 0~4 | 프로젝트 기반, 로그인, 종목 등록, 연관 키워드, 기사 수집 | 완료 | `ea0609c` |
| 5 | Jev 기사 평가 | 완료 | `82e49d7` |
| 6 | 정기 수집, 누락 보정, 키워드 자동 재생성 | 완료 | `895819c` |
| 7 | 타임라인과 필터 | 완료 | `53e0df7` |
| 8 | 설정 화면, 사용량, 비용 한도 | 완료 | `b121a23` |
| 9 | Cloudflare Tunnel, 실행 안내, 백업 | 진행 중(확인 끝, 보고 전) | — |

## 3. 일하는 규칙 (CLAUDE.md에 더해 사용자가 요청한 것)

- **단계 시작 = 이전 단계 커밋**: 새 단계를 시작할 때 이전 단계 변경을 main에 커밋한다. 커밋 전에 `.env`의 비밀 값과 흔한 키 형식이 스테이징 내용에 없는지 검사하고 `.env`가 스테이징되지 않았는지 본다. 푸시는 요청할 때만.
- **질문 → 기록 → 구현**: 단계 시작 전에 그 단계의 미결정(D-xx)과 화면·비용·구조 관련 질문을 모아 A/B·장단점·추천으로 묻는다. 답은 SPEC 12장(13장 상태)에 먼저 적고 구현한다.
- **작은 결정**: 스스로 정한 것은 SPEC 12장에 "— 구현 중 정함"으로 적고 보고에 표로 알린다. 사용자가 이의 없이 다음 응답을 하면 "— 구현 중 정함, 보고 후 이의 없음"으로 바꾼다.
- **돈이 드는 실행**: 예상 비용을 말하고 승인받는다. 승인과 실제 사용은 SPEC 12장 "N단계 실제 API 호출" 줄과 아래 5장에 적는다. 개발 중 worker 밖에서 직접 부른 AI 호출도 사용량 기록(`usage_logs`)에 `*_check` 용도로 넣어 월 합계를 맞춘다.
- **본 Docker에는 사용자 실제 데이터가 있다(2026-10-21부터)**: compose 프로젝트 `ripple`(볼륨 `ripple_ripple-data`)은 사용자가 쓰고 있다. 실제 확인은 시험용 Docker(프로젝트 `ripple-check`, 아래 4장)로 한다. 본 Docker의 비밀번호·설정·종목·예산을 바꾸지 않고, 재빌드(배포)는 사용자가 요청할 때만 한다. 읽기 전용 조회는 괜찮다.
- **문서화**: 이 문서를 단계 중간에도 계속 고친다(답을 받았을 때, 구현이 끝났을 때, 검사·확인 뒤, 보고 뒤, 커밋 뒤). 세션 전용 임시 폴더(scratchpad)에 둔 것은 새 세션에서 사라지므로, 다시 필요할 방법·절차는 이 문서에 적는다.
- 답변은 한국어로, 결론 먼저, 비교는 표로.

## 4. 실행·검사

| 명령 | 내용 |
|---|---|
| `pnpm lint` / `pnpm typecheck` / `pnpm format:check` | 검사 |
| `pnpm test` | Vitest(실제 마이그레이션으로 만든 임시 DB) |
| `pnpm test:e2e` | Playwright. 가짜 외부 서비스(4200), 시험용 worker(4100)·web(3100)을 띄운다. `.e2e/ripple.db`를 새로 만든다 |
| `pnpm build` | Next 빌드 |
| `docker compose up -d --build` | 본 Docker(사용자 것, 위 규칙 참고) |

E2E에서 알아 둘 것
- 시험용 worker는 `SCHEDULER_DISABLED=1`로 정기 작업을 끈다(지금 수집과 겹치지 않게).
- 타임라인·설정 테스트는 시험 DB에 데이터를 직접 넣는다(멜콘·나노솔루션 종목, 사용량 4.2달러). 앱 전체 값을 바꾸는 테스트는 데스크톱에서만 차례로 돌고 바로 되돌린다.
- 로그인 테스트 헬퍼(`e2e/helpers.ts`의 `submitPassword`)는 앞 제출이 끝날 때까지 기다린 뒤 비밀번호를 넣는다. React가 제출이 끝난 뒤 폼을 비우는데, 그 전에 넣은 값이 지워져 브라우저가 빈 칸 제출을 막던 것이 7·9단계 간헐 실패(30번 중 1번꼴)의 원인이었다. 고친 뒤 140번 되풀이해 실패 0.
- 되풀이 실행: `pnpm exec playwright test e2e/auth.spec.ts --repeat-each=10`(접속자 라벨에 되풀이 번호가 붙어 실패 제한이 섞이지 않음).

시험용 Docker(본 Docker와 볼륨·포트·백업 폴더를 나눔, 2026-10-21 9단계에서 씀)
- 띄우기: `WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups OPENROUTER_API_KEY= TYPESAFE_API_KEY= FINNHUB_API_KEY= APP_PASSWORD=… SESSION_SECRET=… docker compose -p ripple-check up -d --build`
  - AI 키를 비워 비용이 생기지 않게 한다. 터널까지 뜨므로 공개 주소가 생긴다. 외부에서 확인할 때는 공개된 시험 비밀번호(`e2e/test-env.ts`) 대신 무작위 비밀번호를 파일로 만들어 쓰고(출력하지 않음) 끝나면 지운다.
  - 임시 주소: `docker compose -p ripple-check logs tunnel | grep trycloudflare` 또는 http://127.0.0.1:3300/status
- 내리기: `WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups docker compose -p ripple-check down -v`, `rm -rf .check-backups`, 시험 이미지 `docker image rm ripple-check-web ripple-check-worker ripple-check-migrate`
- 복원 시험: `down` → 시험 볼륨(`ripple-check_ripple-data`)에 README의 복원 명령 → `up -d`

## 5. 데이터·비용 메모

- 본 Docker DB(2026-10-21 17:49 조회): 사용자 종목 두산에너빌리티(id 4). 시험 종목 삼성전자·엔비디아·현대건설(id 1~3)은 숨김 상태(deleted_at 1792465169686·…692·…693). 시험에 다시 쓰지 않는다.
- 이번 달 AI 사용: $0.0595(한도 $5, 개발 중 시험 호출 포함).
- 단계별 실제 API 사용: 2단계 OpenRouter 약 20회, 3단계 약 $0.016(누적), 5단계 Jev $0.0195(548회), 6단계 $0.0205(OpenRouter 15회, Jev 339회), 7·8단계 0원.

## 6. 다시 잴 때의 방법

세션 임시 폴더의 스크립트는 남아 있지 않으므로 아래 절차로 다시 만든다. 결과는 SPEC 11장에 적는다.

- **Jev 지시문 언어·모델 버전 비교(5단계, D-23)**: 시험 종목과 키워드를 새 임시 DB에 넣고 `startCollectionRun`(decider 없이)으로 실제 기사를 모은다 → 연결을 연결 방식·단계별로 섞어 50건 뽑는다 → 사람이 관련도(0~5)·호재/악재를 먼저 판정한다 → `createTypeSafeDecider`로 언어 2 × 버전 2를 부른다 → 6점 기준 일치, 평균 오차, 호재·악재 일치, 입력 토큰, 지연을 비교한다. 사용량은 `article_evaluation_check`로 본 DB에 옮긴다.
- **키워드 프롬프트 비교(6단계)**: 같은 DB를 둘로 복사해 한쪽에만 평가를 넣고(최근 기사 제목이 생기게) `generateKeywordsForStock`을 종목마다 두 번씩 부른다 → 트리를 나란히 비교한다. 사용량은 `keyword_generation_check`.
- **피드에 담긴 기간(4·6단계, D-01 근거)**: 피드마다 받은 기사 중 가장 최근과 가장 오래된 발행 시각의 차이를 잰다. 바쁜 피드는 약 2시간치다.

## 7. 남은 일·알려진 문제

- 키워드를 매일 다시 만들면 10개 중 0~6개만 겹친다(제목 없이도 같음). 바뀌는 게 부담되면 설정에서 주 1회로.
- 최근 기사 제목을 넣으면 LLM이 단계별 5개를 넘겨 답하는 일이 있다(검증에서 버려 트리는 정상, 출력 토큰만 낭비).
- 같은 소식을 다른 제목으로 낸 기사는 묶이지 않는다(완전히 같은 제목만 묶음).
- 새로 등록한 종목의 지난 24시간 기사 연결은 그 회차에 실패한 소스에서는 놓칠 수 있다.
- MVP 범위 밖(SPEC 2.2): 요약, 외부 알림, 미국 증권사 리포트, 다중 사용자, 종목별 수집 간격, 고정 도메인.

## 8. 코드 지도

폴더 구조는 README "폴더 구조". 자주 보는 진입점:
- worker 시작과 의존성 조립: `src/worker/index.ts`, 내부 API: `src/worker/server.ts`
- 정기 작업: `src/server/schedule/`(plan = 다음 시각 계산, scheduler = 1분 루프)
- 수집: `src/server/collect/run.ts`, 평가: `src/server/evaluate/run.ts`, Jev: `src/server/ai/decider/`
- 화면: `src/app/(app)/`(page = 타임라인, status/, stocks/, settings/)
- 설정 값 기본: `src/server/settings.ts`, 설정 화면 검사·저장: `src/server/settings-form.ts`
