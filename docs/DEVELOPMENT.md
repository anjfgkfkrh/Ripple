# 개발 현황과 이어서 하기

새 세션은 이 문서의 1장부터 읽는다. 이 문서는 짧게 둔다(지금 상태, 다음 할 일, 작업 방법). 지난 기록은 DECISIONS.md로 보낸다.

| 문서 | 내용 | 언제 읽나 |
|---|---|---|
| `CLAUDE.md` | 일하는 규칙 | 자동으로 읽힘 |
| [SPEC.md](../SPEC.md) | 기능 명세, 미결정 목록(13장) | 작업에 관련된 장만 |
| 이 문서 | 지금 상태, 다음 할 일, 작업 방법 | 세션 시작 때 1장, 나머지는 필요할 때 |
| [DECISIONS.md](DECISIONS.md) | 구현 세부 결정(1장), 실측(2장), 지난 진행 기록(3장) | 필요할 때 찾아서 |
| [README.md](../README.md) | 설치·사용법 | 필요할 때 |

"지금 상태"가 실제와 다르면 git log와 `git status`를 먼저 본다.

## 1. 지금 상태

마지막 갱신: 2026-10-30 (한국 시간), 개발 환경 정리 커밋 끝, 토픽 유지 개선 구현 시작 전

- **토픽 유지 개선(2026-10-30, 사용자 결정 1A 2A 3A 4A 5승인 → DECISIONS.md 1장 "토픽 유지 판단" 줄, 근거는 2장 "토픽 유지 시뮬레이션")**: 결정만 끝, 구현 전. 로그·본 DB 분석에서 토픽이 너무 자주 바뀌는 원인 3개를 찾았다(지난 갱신 뒤 새 제목만 보고 끝냄, 토픽 수 하한 10개 강제, 끝난 토픽은 이어 붙이지 못함).
  - 구현할 것: ① `topics/run.ts`: 갱신 때 진행 중 토픽마다 최근 24시간에 걸린 기사 수(`article_matches`)를 세고, AI가 목록에서 뺀 토픽은 5건 미만일 때만 끝냄. 15개를 넘으면 화면 기준 통과 기사가 적은 것부터 끝냄 ② `topics/refresh.ts`: 프롬프트 "10~15개" → "최대 15개, 없으면 적게", 이전 토픽 줄에 24시간 기사 수 덧붙임 ③ 끝난 지 48시간 안의 토픽도 이전 토픽 목록에 넣어 같은 주제면 같은 id로 되살림(`activeAutoTopics`는 진행 중만 돌려줌, `continueTopics`의 이름·검색어 이음 규칙도 이 목록으로) ④ 기준 건수는 상수. 단위 테스트를 더하고 `/check`
  - 유료 비교 승인(약 $0.08, Haiku 토픽 갱신만): 구현 뒤 지난 갱신 몇 회의 입력(제목·이전 토픽)을 지금 방식·새 방식으로 다시 돌려 토픽 수·이어짐·끝남을 비교. 사용량은 `topic_refresh_check`로 본 DB에 넣음(`/real-check` 절차, 본 DB는 건드리지 않음)
  - 분석 스크립트: `scripts/analysis/`(README 참고, 별도 커밋)
  - 이 작업과 별개로 AI 비용 줄이기(아래)의 답 대기 항목 중 "토픽 수·갱신 간격"은 6시간 간격(10-29 사용자가 설정에서 바꿈)으로 이미 바뀌어 있다.
- **단계**: 0~12단계 완료. 12단계(읽은 기사 표시)까지 본 Docker에 배포됨(2026-10-28 18:07, 커밋 `05930df`). 12단계 휴대폰·PC 화면 확인 결과는 기록이 없다. 다음 단계는 아직 정하지 않았다.
- **개발 환경 정리(2026-10-29, 결정 DECISIONS.md 1장 "개발 환경" 줄)**: 끝, `/stage-start`로 2026-10-30 main에 커밋(사용자 이의 없음). 문서 나누기, CLAUDE.md(`@AGENTS.md`), `.claude/`(설정·hook 2개·skill 7개), `scripts/check-secrets.mjs`와 테스트, README. 검사: 서식·린트·타입·단위 423개·빌드·E2E 37개 통과.
  - 사용자가 할 일(남음): ① `sudo chown -R "$(whoami)":staff ~/.npm`(playwright MCP) ② GitHub 토큰을 `GITHUB_PERSONAL_ACCESS_TOKEN`으로 셸 환경에 넣기(github MCP) ③ 새 세션에서 `/mcp`로 확인.
- **사용자 답 대기: 키워드·채점 점검의 개선안 I1~I9(2026-10-30, DECISIONS.md 2장 "키워드·채점 점검")**: 코드 오류 B1·B2 수정과 개선 후보. I4·I6·I8은 아래 AI 비용 질문과 겹치므로 합쳐 A/B·추천으로 다시 묻는다.
- **사용자 답 대기: AI 비용 줄이기(2026-10-23, 무료 조사만, 구현 전)**: 본 DB 사용량으로 월 약 $2.6 추정. 조사 결과는 DECISIONS.md 2장 "AI 비용 줄이기 조사".
  - 사용자에게 물은 것: 토픽 갱신 모델 교체(품질 비교 유료), 일본어 제목 빼기, Jev 질문 문구 줄이기(유료 비교), 시장·토픽을 기사당 한 번에 묻기, 키워드 재생성 매주, 토픽 수·갱신 간격, 목표 금액. 답을 받으면 DECISIONS.md 1장에 적고 시작
- **다음**: 토픽 유지 개선 구현(위). 그 뒤 다음 단계(MVP 범위 밖 기능 또는 AI 비용 줄이기)를 정한다.
- 지난 진행 기록(단계별 체크리스트·배포 기록)은 DECISIONS.md 3장.

## 2. 단계별 현황

| 단계 | 내용 | 상태 | 커밋 |
|---|---|---|---|
| 0~4 | 프로젝트 기반, 로그인, 종목 등록, 연관 키워드, 기사 수집 | 완료 | `ea0609c` |
| 5 | Jev 기사 평가 | 완료 | `82e49d7` |
| 6 | 정기 수집, 누락 보정, 키워드 자동 재생성 | 완료 | `895819c` |
| 7 | 타임라인과 필터 | 완료 | `53e0df7` |
| 8 | 설정 화면, 사용량, 비용 한도 | 완료 | `b121a23` |
| 9 | Cloudflare Tunnel, 실행 안내, 백업 | 완료 | `f972e3c` |
| - | v0.1.0 릴리스 준비, 릴리스 묶음 스크립트 | 완료 | `575bef2`, `0e73b4e` |
| 10 | 키워드 트리에 시사 반영 | 완료(2026-10-22 배포) | `32f4c0d` |
| 11 | 시장 전체 기사(토픽 우선: 자동 토픽·한국 증시·사용자 주제), Jev 점수로 트리·기사 고르기, 비슷한 제목 묶기, 항목별 비용 | 완료(2026-10-22 배포) | `4626733`, 보완 `dd0faf5` |
| 12 | 읽은 기사 표시(링크 클릭·버튼, 서버 저장, 묶인 카드 단위) | 완료(2026-10-28 배포) | `05930df` |
| - | 개발 환경 정리(문서 나누기, `.claude/`, 비밀 값 검사) | 완료(2026-10-30 커밋) | 이 커밋 |

릴리스: **v0.1.0 (MVP)**, 태그 `v0.1.0`. **v0.2.0**(2026-10-23, 10·11단계: 시사·시장 토픽·Jev 점수 기반·같은 소식 묶기·항목별 비용), 태그 `v0.2.0`, GitHub 릴리스 페이지에 `gh release create`로 올림(이제 `gh`가 있음, 커밋 `4626733`).

## 3. 일하는 규칙 (CLAUDE.md에 더해 사용자가 요청한 것)

- **단계 시작 = 이전 단계 커밋**(`/stage-start`): 새 단계를 시작할 때 이전 단계 변경을 main에 커밋한다. 커밋 전에 `node scripts/check-secrets.mjs`로 `.env`의 비밀 값과 흔한 키 형식이 스테이징 내용에 없는지, `.env`가 스테이징되지 않았는지 본다. 푸시는 요청할 때만.
- **질문 → 기록 → 구현**: 단계 시작 전에 그 단계의 미결정(SPEC 13장 D-xx)과 화면·비용·구조 관련 질문을 모아 A/B·장단점·추천으로 묻는다. 답은 DECISIONS.md 1장(과 SPEC 13장 상태)에 먼저 적고 구현한다. 기능이 바뀌면 SPEC 본문도 고친다.
- **작은 결정**: 스스로 정한 것은 DECISIONS.md 1장에 "— 구현 중 정함"으로 적고 보고에 표로 알린다. 사용자가 이의 없이 다음 응답을 하면 "— 구현 중 정함, 보고 후 이의 없음"으로 바꾼다.
- **돈이 드는 실행**: 예상 비용을 말하고 승인받는다. 승인과 실제 사용은 DECISIONS.md 1장 "N단계 실제 API 호출" 줄과 아래 5장에 적는다. 개발 중 worker 밖에서 직접 부른 AI 호출도 사용량 기록(`usage_logs`)에 `*_check` 용도로 넣어 월 합계를 맞춘다.
- **본 Docker에는 사용자 실제 데이터가 있다(2026-10-21부터)**: compose 프로젝트 `ripple`(볼륨 `ripple_ripple-data`)은 사용자가 쓰고 있다. 실제 확인은 시험용 Docker(프로젝트 `ripple-check`, `/real-check`)로 한다. 본 Docker의 비밀번호·설정·종목·예산을 바꾸지 않고, 재빌드(배포)는 사용자가 요청할 때만 한다. 읽기 전용 조회는 괜찮다.
- **세션**: 작업 하나에 세션 하나(결정·구현·검사·배포·질문). 시작할 때 이 문서 1장과 작업에 관련된 SPEC 장만 읽는다. 끝날 때 `/handoff`로 이 문서 1장과 DECISIONS.md를 고치고, 사용자가 `/rename` 뒤 `/clear` 한다. 대화와 세션 임시 폴더(scratchpad)에만 있는 것은 다음 세션에서 사라지므로, 다시 필요한 방법·절차는 문서나 skill에 적는다.
- 답변은 한국어로, 결론 먼저, 비교는 표로.

## 4. 실행·검사

| 명령 | 내용 |
|---|---|
| `pnpm lint` / `pnpm typecheck` / `pnpm format:check` | 검사 |
| `pnpm test` | Vitest(실제 마이그레이션으로 만든 임시 DB) |
| `pnpm test:e2e` | Playwright. 가짜 외부 서비스(4200), 시험용 worker(4100)·web(3100)을 띄운다. `.e2e/ripple.db`를 새로 만든다 |
| `pnpm build` / `pnpm build:worker` | Next 빌드 / worker 번들 |
| `node scripts/check-secrets.mjs` | 스테이징한 변경에 `.env`의 비밀 값·흔한 키 형식이 없는지 본다(값은 출력하지 않음). `--history`는 모든 커밋 기록(푸시 전) |
| `/check` | 위 검사를 모두 돌리고 결과 표를 돌려준다(따로 떨어진 컨텍스트에서 실행). `/check 빠르게`는 서식·린트·타입·단위만 |
| `/deploy` | 본 Docker 배포(백업 → 다시 빌드 → 확인). 사용자가 요청할 때만, 사용자가 직접 부른다 |

Claude Code 설정(2026-10-29, 결정은 DECISIONS.md 1장 "개발 환경" 줄)
- `.claude/settings.json`: 이 프로젝트에서 superpowers 끔, 검사 명령 허용, 푸시·본 Docker 명령은 확인, `.env` 읽기·고치기 막음, hook 두 개(`.claude/hooks/`: 고친 파일 Prettier 서식, Bash 명령의 `.env` 막기)
- `.claude/skills/`: `/check`, `/deploy`, `/stage-start`, `/real-check`, `/handoff`, 디버깅(`debugging`), 완료 전 확인(`before-done`)
- 세션: 작업 하나에 세션 하나. 끝날 때 `/handoff` → `/rename 이름` → `/clear`. 돌아올 때 `/resume`

E2E에서 알아 둘 것
- 시험용 worker는 `SCHEDULER_DISABLED=1`로 정기 작업을 끈다(지금 수집과 겹치지 않게).
- 타임라인·설정 테스트는 시험 DB에 데이터를 직접 넣는다(멜콘·나노솔루션 종목, 사용량 4.2달러). 앱 전체 값을 바꾸는 테스트는 데스크톱에서만 차례로 돌고 바로 되돌린다.
- 로그인 테스트 헬퍼(`e2e/helpers.ts`의 `submitPassword`)는 앞 제출이 끝날 때까지 기다린 뒤 비밀번호를 넣는다. React가 제출이 끝난 뒤 폼을 비우는데, 그 전에 넣은 값이 지워져 브라우저가 빈 칸 제출을 막던 것이 7·9단계 간헐 실패(30번 중 1번꼴)의 원인이었다. 고친 뒤 140번 되풀이해 실패 0.
- 되풀이 실행: `pnpm exec playwright test e2e/auth.spec.ts --repeat-each=10`(접속자 라벨에 되풀이 번호가 붙어 실패 제한이 섞이지 않음).

시험용 Docker(본 Docker와 볼륨·포트·백업 폴더를 나눔, 2026-10-21 9단계에서 씀). 백업 사본을 넣는 법과 실제 AI 확인까지 담은 절차는 `/real-check`
- 띄우기: `WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups OPENROUTER_API_KEY= TYPESAFE_API_KEY= FINNHUB_API_KEY= APP_PASSWORD=… SESSION_SECRET=… docker compose -p ripple-check up -d --build`
  - AI 키를 비워 비용이 생기지 않게 한다. 터널까지 뜨므로 공개 주소가 생긴다. 외부에서 확인할 때는 공개된 시험 비밀번호(`e2e/test-env.ts`) 대신 무작위 비밀번호를 파일로 만들어 쓰고(출력하지 않음) 끝나면 지운다.
  - 임시 주소: `docker compose -p ripple-check logs tunnel | grep trycloudflare` 또는 http://127.0.0.1:3300/status
- 내리기: `WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups docker compose -p ripple-check down -v`, `rm -rf .check-backups`, 시험 이미지 `docker image rm ripple-check-web ripple-check-worker ripple-check-migrate`
- 복원 시험: `down` → 시험 볼륨(`ripple-check_ripple-data`)에 README의 복원 명령 → `up -d`

## 5. 데이터·비용 메모

- 본 Docker DB(2026-10-21 17:49 조회): 사용자 종목 두산에너빌리티(id 4). 시험 종목 삼성전자·엔비디아·현대건설(id 1~3)은 숨김 상태(deleted_at 1792465169686·…692·…693). 시험에 다시 쓰지 않는다.
- 본 Docker DB(2026-10-22 조회): 사용자 종목 현대건설(id 3, 2026-10-21 저녁 다시 등록)·두산에너빌리티(id 4), 깊이 3. 삼성전자·엔비디아(id 1·2)는 숨김.
- 이번 달 AI 사용: $0.1424(2026-10-22, 한도 $5, 개발 중 시험 호출 포함).
- 단계별 실제 API 사용: 2단계 OpenRouter 약 20회, 3단계 약 $0.016(누적), 5단계 Jev $0.0195(548회), 6단계 $0.0205(OpenRouter 15회, Jev 339회), 7·8단계 0원, 10단계 실측 $0.0281(OpenRouter 10회, Jev 80회)과 실제 AI 확인 $0.0132(OpenRouter 3회, Jev 88회), 10단계 보완 실측 $0.0077(OpenRouter 4회), 11단계 실측 $0.0241(OpenRouter 5회, Jev 137회), 모두 본 DB 사용량 기록에 `*_check`로 넣음. 11단계 무료 실제 확인은 0원, 실제 AI 확인 $0.0284(OpenRouter 5회, Jev 300회, 본 DB에 `*_check`로 옮김). 본 DB 이번 달 합계 약 $0.186(배포 직후).

## 6. 다시 잴 때의 방법

DECISIONS.md 2장 "다시 잴 때의 방법"으로 옮겼다. 시험용 Docker 안 실제 AI 확인은 `/real-check`.

## 7. 남은 일·알려진 문제

- 키워드를 매일 다시 만들면 10개 중 0~6개만 겹친다(제목 없이도 같음). 바뀌는 게 부담되면 설정에서 주 1회로.
- 최근 기사 제목을 넣으면 LLM이 단계별 5개를 넘겨 답하는 일이 있다(검증에서 버려 트리는 정상, 출력 토큰만 낭비).
- 같은 소식이라도 언론사마다 제목이 많이 다르면(겹침 0.2~0.35) 묶이지 않는다(11단계 기준은 '…' 앞부분 0.5 또는 전체 0.4, DECISIONS.md 2장).
- (11단계) Jev는 키워드·토픽 점수를 사람보다 후하게 매긴다(대부분 6~9점, DECISIONS.md 2장). 기준(6·5)으로 남는 키워드가 많으면 설정에서 올린다.
- (11단계) AI가 키워드 후보를 요청(단계마다 5~8개)보다 훨씬 많이 낼 때가 있다(한국 증시 112개). 100개에서 자르고 Jev 평가 뒤 40개 상한으로 남긴다(출력·평가 비용만 늘어남).
- (11단계) 한국 증시 기사가 많이 보일 수 있다(실제 확인: 하루 평가 183건 중 40건이 기준 이상, 한은 금리 결정 날). 많으면 설정에서 영향도 기준을 올린다.
- (11단계) 토픽 갱신에서 AI가 이전 토픽 id를 돌려주지 않을 때가 있어 이름·검색어가 같으면 같은 토픽으로 잇는다. 이름과 검색어가 모두 바뀌면 새 토픽이 되고 이전 토픽은 끝난다.
- 새로 등록한 종목의 지난 24시간 기사 연결은 그 회차에 실패한 소스에서는 놓칠 수 있다.
- (10단계) 시장 이슈 정리는 회사 한 곳의 큰 소식(실적·신차)도 넣고, 기사가 흩어진 주제(예: 트럼프 기사 하루 43건)는 이슈로 따로 잡지 않기도 한다. 시사 키워드 중 넓은 말('기준금리 동결' 하루 17건, 'AI 투자 우려' 15.5건)은 연결이 많지만 대부분 기준 미만이다. 큰 사건 때 시사 연결이 6점을 넘는지는 아직 재지 못했다(DECISIONS.md 2장).
- (고침, 2026-10-23) 국민일보 피드가 다음 날 지면 기사에 4~24시간 미래 시각을 붙여 와 타임라인에 미래 시각으로 보였다. 받은 시각보다 10분 넘게 미래면 받은 시각으로 저장하고, 저장된 것은 마이그레이션 0008로 고침(DECISIONS.md 1장 "미래 발행 시각"). 2026-10-23 11:59 배포, 본 DB 기사 14건·제목 126건 고침
- MVP 범위 밖(SPEC 2.2): 요약, 외부 알림, 미국 증권사 리포트, 다중 사용자, 종목별 수집 간격, 고정 도메인.

## 8. 코드 지도

폴더 구조는 README "폴더 구조". 자주 보는 진입점:
- worker 시작과 의존성 조립: `src/worker/index.ts`, 내부 API: `src/worker/server.ts`
- 정기 작업: `src/server/schedule/`(plan = 다음 시각 계산, scheduler = 1분 루프)
- 수집: `src/server/collect/run.ts`, 평가: `src/server/evaluate/run.ts`, Jev: `src/server/ai/decider/`
- 키워드: `src/server/keywords/`(tree = 프롬프트·검증·점수로 고르기(구조 키워드만), generate = 생성·Jev 평가·저장, queue = 대기열, headlines = 종목의 최근 기사 제목), 피드 제목 보관: `src/server/collect/feed-headlines.ts`, 지난 기사 걸기: `src/server/collect/backfill.ts`
- 시장·주제·토픽(11단계): `src/server/topics/`(store = 항목·토픽 저장과 가지 붙이기, plan = 갱신 시각, refresh = 프롬프트, run = 갱신 실행, read = 화면 읽기), 같은 소식 기준: `src/server/similar-title.ts`, 토픽 카드: `src/server/timeline/read.ts`의 `readTopicCards`
- 화면: `src/app/(app)/`(page = 타임라인, status/, stocks/, settings/)
- 읽은 기사 표시(12단계): 저장·조회 `src/server/timeline/reads.ts`, 카드의 `read`는 `timeline/read.ts`, worker `PUT /articles/read`(`src/worker/server.ts`), 서버 액션 `setArticlesReadAction`(`src/app/(app)/actions.ts`), 카드 겉틀 `src/app/(app)/read-card.tsx`
- 설정 값 기본: `src/server/settings.ts`, 설정 화면 검사·저장: `src/server/settings-form.ts`
