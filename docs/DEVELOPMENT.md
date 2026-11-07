# 개발 현황과 이어서 하기

> 읽는 때: 단계 표(2장), 일하는 규칙(3장), 실행·검사(4장), 코드 지도(8장) 등 작업 방법이 필요할 때 그 장만.

지금 상태는 [STATE.md](STATE.md)에 있다. 이 문서는 작업 방법을 둔다. 지난 기록은 DECISIONS.md로 보낸다.

| 문서 | 내용 | 언제 읽나 |
|---|---|---|
| `CLAUDE.md` | 일하는 규칙 | 자동으로 읽힘 |
| [STATE.md](STATE.md) | 지금 상태, 답 대기, 다음 할 일 | 세션 시작 때 |
| [SPEC.md](SPEC.md) | 기능 명세, 미결정 목록(13장) | 작업에 관련된 장만 |
| 이 문서 | 단계 표, 작업 방법, 코드 지도 | 필요한 장만 |
| [DECISIONS.md](DECISIONS.md) | 구현 세부 결정(1장), 실측(2장), 지난 진행 기록(3장) | 필요할 때 찾아서 |
| [README.md](../README.md) | 설치·사용법 | 필요할 때 |

## 1. 지금 상태

[STATE.md](STATE.md)로 옮겼다(2026-11-07, agentflow 문서 구조). 장 번호는 다른 문서가 가리키므로 그대로 둔다.

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
| - | 개발 환경 정리(문서 나누기, `.claude/`, 비밀 값 검사) | 완료(2026-10-30 커밋) | `4c4f259` |
| - | 토픽 유지 개선(24시간 기사 수로 유지, 끝난 토픽 되살림, 최대 토큰 10,000) | 완료(2026-10-30 배포) | `46e3f58` |
| 13 | 보유 종목 주가 띠(한국 KIS, 미국 Finnhub, 장중 1분, 이력) | 완료(2026-10-30 배포) | `ddde1a5` |
| 14 | Rust 전환(기능 변경 없음, R0~R13, `docs/RUST_MIGRATION.md`) | 완료(2026-11-07 배포) | 설계 `92e956d`, R0 `ef6257d`, R1 `f142410`, R2 `5e03ee5`, R3 `56df2a3`, R4 `88c5c93`, R5 `b35bcca`, R6 `f371b85`, R7 `0055b76`, R8 `1c4f3ce`, R9 `207e4eb`, R10 `b54805b`, R11 `3dece10`, R12 `31d962f`, R13 `bc97679` |
| 15 | 고정 주소(커스텀 도메인, 이름 붙은 터널 토큰, `work/커스텀도메인/`) | 진행 | |

릴리스: **v0.1.0 (MVP)**, 태그 `v0.1.0`. **v0.2.0**(2026-10-23, 10·11단계: 시사·시장 토픽·Jev 점수 기반·같은 소식 묶기·항목별 비용), 태그 `v0.2.0`, GitHub 릴리스 페이지에 `gh release create`로 올림(이제 `gh`가 있음, 커밋 `4626733`). **v0.3.0**(2026-11-07, 12~14단계: 읽음 표시·주가 띠·Rust 전환·수집 TLS 수정), 태그 `v0.3.0`(`06b11d7`), 이미지 하나 arm64·amd64 각 41M. **v0.2.0 이하에서는 업데이트할 수 없다**(사용자 답 B, DECISIONS.md 1장 "v0.3.0 업데이트 경로"). 고치려면 안 A(drizzle 이력 앞부분 k줄 받기).

## 3. 일하는 규칙 (CLAUDE.md에 더해 사용자가 요청한 것)

- **단계는 agentflow로 진행한다(2026-11-07부터)**: `/agentflow:plan <단계>` → `/agentflow:run <단계>` → (사용자가 결과 승인) → `/agentflow:close <단계>`. 단계 작업은 브랜치 `af/<단계>`에서 하고, agentflow 스킬은 그 브랜치에 스스로 커밋한다. main에 합치기·푸시·배포는 사용자가 요청할 때만 한다. 합치기 전에 `node scripts/check-secrets.mjs`(main 기준 바뀐 내용이 스테이징된 상태에서)로 `.env`의 비밀 값과 흔한 키 형식이 없는지, `.env`가 들어가지 않았는지 본다. 하위(worker)는 이 검사를 하지 않고 커밋하므로 합칠 때 한 번 반드시 한다. 단계 밖 작업(배포, 급한 수정, 릴리스)은 지금처럼 main에서 하고 커밋은 요청할 때만. review는 STATE.md만 다시 쓰므로, 2장 단계 표의 줄은 main에 합칠 때(또는 그 뒤 `/handoff`에서) 더한다.
- **질문 → 기록 → 구현**: 단계 시작 전(`/agentflow:plan`에서 작업 목록을 승인받기 전)에 그 단계의 미결정(SPEC 13장 D-xx)과 화면·비용·구조 관련 질문을 모아 A/B·장단점·추천으로 묻는다. 답은 DECISIONS.md 1장(과 SPEC 13장 상태)에 먼저 적고 구현한다. 기능이 바뀌면 SPEC 본문도 고친다.
- **작은 결정**: 스스로 정한 것은 DECISIONS.md 1장에 "— 구현 중 정함"으로 적고 보고에 표로 알린다. 사용자가 이의 없이 다음 응답을 하면 "— 구현 중 정함, 보고 후 이의 없음"으로 바꾼다. 이전 단계의 줄은 다음 단계 `/agentflow:plan`을 시작할 때 바꾼다(전에는 `/stage-start`가 했다).
- **돈이 드는 실행**: 예상 비용을 말하고 승인받는다. 승인과 실제 사용은 DECISIONS.md 1장 "N단계 실제 API 호출" 줄과 아래 5장에 적는다. 개발 중 worker 밖에서 직접 부른 AI 호출도 사용량 기록(`usage_logs`)에 `*_check` 용도로 넣어 월 합계를 맞춘다.
- **본 Docker에는 사용자 실제 데이터가 있다(2026-10-21부터)**: compose 프로젝트 `ripple`(볼륨 `ripple_ripple-data`)은 사용자가 쓰고 있다. 실제 확인은 시험용 Docker(프로젝트 `ripple-check`, `/real-check`)로 한다. 본 Docker의 비밀번호·설정·종목·예산을 바꾸지 않고, 재빌드(배포)는 사용자가 요청할 때만 한다. 읽기 전용 조회는 괜찮다.
- **세션**: 작업 하나에 세션 하나(결정·구현·검사·배포·질문). 단계 작업은 agentflow가 작업마다 하위 세션을 나누고, 인계는 `work/<단계>/<작업>/` 파일로 한다. 단계 밖 세션은 시작할 때 STATE.md와 작업에 관련된 SPEC 장만 읽는다. 끝날 때 `/handoff`로 STATE.md와 DECISIONS.md를 고치고, 사용자가 `/rename` 뒤 `/clear` 한다. 대화와 세션 임시 폴더(scratchpad)에만 있는 것은 다음 세션에서 사라지므로, 다시 필요한 방법·절차는 문서나 skill에 적는다.
- 답변은 한국어로, 결론 먼저, 비교는 표로.

## 4. 실행·검사

| 명령 | 내용 |
|---|---|
| `cd rust && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` | Rust 검사와 단위·통합 테스트(실제 마이그레이션으로 만든 임시 DB) |
| `pnpm typecheck` / `pnpm format:check` / `pnpm lint` | E2E·스크립트의 타입, 서식, ESLint(typescript-eslint) |
| `pnpm test` | Vitest(커밋 전 비밀 값 검사 스크립트의 테스트) |
| `pnpm test:e2e` | Playwright. 가짜 외부 서비스(4200), 시험용 worker(4100)·web(3100)을 띄운다. `.e2e/ripple.db`를 새로 만든다 |
| `cd rust && ./build.sh --release` | Tailwind CSS와 release 실행 파일(E2E가 쓴다, 고친 뒤 다시) |
| `docker build --target ripple .` | 이미지 하나(`ripple`)를 만든다. 본 Docker와 무관(컨테이너를 건드리지 않음) |
| `node scripts/check-secrets.mjs` | 스테이징한 변경에 `.env`의 비밀 값·흔한 키 형식이 없는지 본다(값은 출력하지 않음). `--history`는 모든 커밋 기록(푸시 전) |
| `/check` | 위 검사를 모두 돌리고 결과 표를 돌려준다(따로 떨어진 컨텍스트에서 실행). `/check 빠르게`는 서식·린트·타입·단위만 |
| `/deploy` | 본 Docker 배포(백업 → 다시 빌드 → 확인). 사용자가 요청할 때만, 사용자가 직접 부른다 |

Claude Code 설정(2026-10-29, 결정은 DECISIONS.md 1장 "개발 환경" 줄)
- `.claude/settings.json`: 이 프로젝트에서 superpowers 끔, 검사 명령 허용, 푸시·본 Docker 명령은 확인, `.env` 읽기·고치기 막음, hook 두 개(`.claude/hooks/`: 고친 파일 Prettier 서식, Bash 명령의 `.env` 막기)
- `.claude/skills/`: `/check`, `/deploy`, `/real-check`, `/handoff`(단계 밖 작업), 디버깅(`debugging`), 완료 전 확인(`before-done`)
- agentflow 플러그인(AgentDev 저장소 `agentflow/`, 마켓플레이스 `agentdev`): 단계 작업 `/agentflow:plan`·`run`·`close`, 역할별 모델 `.agentflow/models.json`, 단계 작업 폴더 `work/<단계>/`. 단계를 닫으면 `work/<단계>/`는 보관소 `../Ripple-archive/`(따로 된 git 저장소, `.agentflow/config.json`의 `archive_root`로 바꿀 수 있음)로 옮긴다. 플러그인은 `.claude/settings.local.json`(커밋 안 함)에서 켠다.
- 세션: 작업 하나에 세션 하나. 끝날 때 `/handoff` → `/rename 이름` → `/clear`. 돌아올 때 `/resume`

E2E에서 알아 둘 것
- 시험용 worker는 `SCHEDULER_DISABLED=1`로 정기 작업을 끈다(지금 수집과 겹치지 않게).
- 타임라인·설정 테스트는 시험 DB에 데이터를 직접 넣는다(멜콘·나노솔루션 종목, 사용량 4.2달러). 앱 전체 값을 바꾸는 테스트는 데스크톱에서만 차례로 돌고 바로 되돌린다.
- 로그인 테스트 헬퍼(`e2e/helpers.ts`의 `submitPassword`)는 앞 제출이 끝날 때까지 기다린 뒤 비밀번호를 넣는다. React가 제출이 끝난 뒤 폼을 비우는데, 그 전에 넣은 값이 지워져 브라우저가 빈 칸 제출을 막던 것이 7·9단계 간헐 실패(30번 중 1번꼴)의 원인이었다. 고친 뒤 140번 되풀이해 실패 0.
- 되풀이 실행: `pnpm exec playwright test e2e/auth.spec.ts --repeat-each=10`(접속자 라벨에 되풀이 번호가 붙어 실패 제한이 섞이지 않음).
- Rust web을 확인하는 E2E 전에는 `cd rust && ./build.sh --release`로 release를 새로 빌드한다(옛 바이너리면 `/`가 404, R11에서 겪음).

시험용 Docker(본 Docker와 볼륨·포트·백업 폴더를 나눔, 2026-10-21 9단계에서 씀). 백업 사본을 넣는 법과 실제 AI 확인까지 담은 절차는 `/real-check`
- 띄우기: `TUNNEL_TOKEN= PUBLIC_URL= WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups OPENROUTER_API_KEY= TYPESAFE_API_KEY= FINNHUB_API_KEY= KIS_APP_KEY= KIS_APP_SECRET= APP_PASSWORD=… SESSION_SECRET=… docker compose -p ripple-check up -d --build`
  - `TUNNEL_TOKEN= PUBLIC_URL=`는 모든 시험 compose 명령에 붙인다(15단계). 본 토큰으로 시험 tunnel이 뜨면 Cloudflare가 실제 접속을 시험 Docker로도 나눠 보낸다. 셸의 빈 값이 env 파일 값을 덮는 것은 `config`로 확인함(2026-11-07).
  - 고정 주소 확인은 시험용 터널(`check.<도메인>`)의 값을 저장소 루트 `check-tunnel.vars`(git 제외, 사용자가 만듦)에 두고 `/real-check` 4-1장대로 셸에 읽는다. 본 env 파일에 시험 토큰을 두지 않는다.
  - AI 키를 비워 비용이 생기지 않게 한다. 터널까지 뜨므로 공개 주소가 생긴다. 외부에서 확인할 때는 공개된 시험 비밀번호(`e2e/test-env.ts`) 대신 무작위 비밀번호를 파일로 만들어 쓰고(출력하지 않음) 끝나면 지운다.
  - 임시 주소: `docker compose -p ripple-check logs tunnel | grep trycloudflare` 또는 http://127.0.0.1:3300/status
- 내리기: `TUNNEL_TOKEN= PUBLIC_URL= WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups docker compose -p ripple-check down -v`, `rm -rf .check-backups`(이미지 `ripple-app`은 본 배포와 같은 이름이라 지우지 않는다)
- 복원 시험: `down` → 시험 볼륨(`ripple-check_ripple-data`)에 README의 복원 명령 → `up -d`

## 5. 데이터·비용 메모

- 본 Docker DB(2026-10-21 17:49 조회): 사용자 종목 두산에너빌리티(id 4). 시험 종목 삼성전자·엔비디아·현대건설(id 1~3)은 숨김 상태(deleted_at 1792465169686·…692·…693). 시험에 다시 쓰지 않는다.
- 본 Docker DB(2026-10-22 조회): 사용자 종목 현대건설(id 3, 2026-10-21 저녁 다시 등록)·두산에너빌리티(id 4), 깊이 3. 삼성전자·엔비디아(id 1·2)는 숨김.
- 토픽 유지 비교(2026-10-30): OpenRouter 14회 약 $0.086, 본 DB 이번 달 합계 $1.0364 → $1.1244(`topic_refresh_check` 7줄).
- 이번 달 AI 사용: $0.1424(2026-10-22, 한도 $5, 개발 중 시험 호출 포함).
- 단계별 실제 API 사용: 2단계 OpenRouter 약 20회, 3단계 약 $0.016(누적), 5단계 Jev $0.0195(548회), 6단계 $0.0205(OpenRouter 15회, Jev 339회), 7·8단계 0원, 10단계 실측 $0.0281(OpenRouter 10회, Jev 80회)과 실제 AI 확인 $0.0132(OpenRouter 3회, Jev 88회), 10단계 보완 실측 $0.0077(OpenRouter 4회), 11단계 실측 $0.0241(OpenRouter 5회, Jev 137회), 모두 본 DB 사용량 기록에 `*_check`로 넣음. 11단계 무료 실제 확인은 0원, 실제 AI 확인 $0.0284(OpenRouter 5회, Jev 300회, 본 DB에 `*_check`로 옮김). 본 DB 이번 달 합계 약 $0.186(배포 직후).

- 14단계 R13 실제 확인(2026-11-05, 승인 뒤, 시험 Docker): 종목 등록 1회, 호출 51회, 합계 $0.003319(OpenRouter 2회 $0.001966, Jev 49회 $0.001353). 본 DB 사용량은 옮기지 않기로 함(2026-11-07).

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
- (14단계 R13, 2026-11-05 추천대로) `scripts/analysis/`(일회성 분석 스크립트)는 지웠다. 전부 본 DB를 worker 컨테이너의 node로 읽었는데 이미지에 node가 없어 돌지 않았다. 옛 파일은 커밋 `31d962f`에 남아 있다.
- (14단계 R13, 2026-11-05 추천대로) ESLint는 `typescript-eslint`로 바꿨고 `next`·`eslint-config-next`는 뺐다. Prettier와 vitest(비밀 값 검사 테스트)는 남겼다.
- (14단계 R13) `.claude/settings.json` 허용 목록에 `pnpm build` 두 줄이 남아 있고 `cargo` 줄은 없다. 권한 파일 변경은 자동 승인이 거부돼서 사용자가 고치거나 허락해야 한다.
- (릴리스, 2026-11-07 v0.3.0) `scripts/release.sh`는 Rust 뒤 약 11분(amd64는 에뮬레이션 컴파일이라 arm64보다 4배쯤 걸림). 끝나면 로컬 `ripple-app:버전` 태그는 마지막에 만든 amd64를 가리키니 지운다(본 Docker는 `ripple-app:latest`라 상관없음). **릴리스 전에 직전 릴리스 이미지(`release-dist/ripple-이전버전-images-arm64.tar.gz`)로 빈 볼륨에 DB를 만들고 새 이미지 `ripple migrate`를 돌려 업데이트가 되는지 본다**(v0.3.0은 이 시험에서 v0.2.0 업데이트 불가를 찾음, 사용자 답 B로 그대로 냄). 문서만 고쳤으면 이미지는 다시 빌드하지 않고 `release-dist/INSTALL.md`(`__VERSION__` 바꿈)와 `SHA256SUMS`만 다시 만든다.
- (v0.3.0, 사용자 답 B) v0.2.0 이하 DB는 Rust 마이그레이션이 받지 않는다(`server/db/migrate.rs` `baseline_drizzle`가 drizzle 이력 11줄만 받음). 업데이트를 되살리려면 안 A(앞에서부터 k줄이고 마지막 값이 journal k번째와 같으면 `user_version = k`). DECISIONS.md 1장 "v0.3.0 업데이트 경로".
- MVP 범위 밖(SPEC 2.2): 요약, 외부 알림, 미국 증권사 리포트, 다중 사용자, 종목별 수집 간격(고정 도메인은 15단계로 들어감).

## 8. 코드 지도

폴더 구조는 README "폴더 구조". 14단계 R13 뒤 서버 코드는 Rust만 있다. 자주 보는 진입점(모두 `rust/ripple/src/`):
- 실행 파일: `main.rs`(하위 명령 migrate · worker · web · healthcheck), worker 조립·시작: `worker/index.rs`, 내부 API: `worker/server.rs`, 마이그레이션 진입점: `worker/migrate.rs`, 마이그레이션 목록: `server/db/migrate.rs`(`MIGRATION_SQL`)
- 정기 작업: `server/schedule/`(plan = 다음 시각 계산, scheduler = 1분 루프)
- 수집: `server/collect/run.rs`, 평가: `server/evaluate/run.rs`, Jev: `server/ai/decider/`
- 키워드: `server/keywords/`(tree = 프롬프트·검증·점수로 고르기(구조 키워드만), generate = 생성·Jev 평가·저장, queue = 대기열, headlines = 종목의 최근 기사 제목), 피드 제목 보관: `server/collect/feed_headlines.rs`, 지난 기사 걸기: `server/collect/backfill.rs`
- 시장·주제·토픽(11단계): `server/topics/`(store = 항목·토픽 저장과 가지 붙이기, plan = 갱신 시각, refresh = 프롬프트, run = 갱신 실행, read = 화면 읽기), 같은 소식 기준: `server/similar_title.rs`, 토픽 카드: `server/timeline/read.rs`의 `read_topic_cards`
- 화면(axum): `web/`(index = 조립, login·status·timeline·stocks·settings = 각 화면, assets = 정적 파일, guard = 로그인 확인, layout = 공통 틀), 템플릿 `templates/`(Askama, htmx 조각)
- 읽은 기사 표시(12단계): 저장·조회 `server/timeline/reads.rs`, 카드의 `read`는 `server/timeline/read.rs`, worker `PUT /articles/read`(`worker/server.rs`), 화면 `web/timeline.rs`, 카드 `templates/timeline/card.html`
- 주가(13단계): `server/prices/`(kis = KIS 클라이언트, plan = 장 시간, run = 1분마다 받기, store = 저장·띠 읽기), Finnhub 현재가는 `server/collect/finnhub.rs`, 띠 화면은 `templates/timeline/`
- 설정 값 기본: `server/settings.rs`, 설정 화면 검사·저장: `server/settings_form.rs`, 화면 `web/settings.rs`, `templates/settings/`
- 종목·시장·주제 화면과 POST(14단계 R12): `web/stocks.rs`, `templates/stocks/`, 확인창과 폴링 동작 `static/app.js`
- 시험 자료: `rust/ripple/tests/fixtures/`(표본 파일, TS 시절 drizzle 고정본 `drizzle/`·`applied.sql` = 업그레이드 시험), 시험 도우미 `test_support.rs`
- 운영: `scripts/release.sh`(이미지 하나를 두 종류로 빌드)
