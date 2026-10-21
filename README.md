# Ripple

보유 주식 관련 뉴스를 자동으로 모으고 Jev(TypeSafe AI)로 관련도와 호재·악재를 골라 웹으로 보여 주는 개인용 앱. 집 노트북의 Docker에서 돌고, 휴대폰에서는 Cloudflare 임시 주소로 접속한다.

- 요구사항·결정 기록: [SPEC.md](SPEC.md)
- 개발 현황과 이어서 하는 방법: [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)

## 하는 일

1. 종목을 등록하면(예: 삼전, NVDA) AI가 연관 키워드 트리를 만든다(매일 06:00에 최근 뉴스를 참고해 다시 만듦).
2. 30분마다 언론사 RSS 29곳과 Finnhub(미국 종목)에서 종목 이름·키워드가 들어간 기사를 모은다.
3. Jev가 기사마다 관련도(0~10)와 호재·악재·중립을 매긴다.
4. 타임라인(홈)에 관련도 6점 이상인 기사를 최신순으로 보여 준다. 종목·호재/악재로 거를 수 있다.
5. 매일 04:00에 DB를 이 컴퓨터의 `backups` 폴더에 백업한다.

## 1. 처음 설치

준비물: Docker Desktop(compose v2). 개발할 때만 Node.js 24와 pnpm 12가 더 필요하다.

```bash
cp .env.example .env    # 아직 .env가 없을 때만
```

`.env`를 열어 값을 넣는다.

| 이름 | 필요 | 내용 |
|---|---|---|
| `APP_PASSWORD` | 필수 | 로그인 비밀번호(7자 이상). 공개 주소로 열리므로 남이 짐작하기 어렵게 |
| `SESSION_SECRET` | 필수 | 32자 이상 무작위 값: `openssl rand -base64 48` |
| `WORKER_API_TOKEN` | 필수 | 32자 이상 무작위 값(web↔worker 내부 토큰): `openssl rand -base64 48` |
| `OPENROUTER_API_KEY` | 필수 | 종목 찾기·키워드 만들기(https://openrouter.ai). 키에 사용 한도를 걸어 두길 권장 |
| `TYPESAFE_API_KEY` | 필수 | 기사 평가 Jev(https://console.typesafe.ai). 없으면 기사는 모으고 평가는 미룬다 |
| `FINNHUB_API_KEY` | 선택 | 미국 종목 뉴스(무료 가입 https://finnhub.io). 없으면 미국 뉴스는 건너뛴다 |

그리고 켠다(처음엔 이미지를 만드느라 몇 분 걸린다).

```bash
docker compose up -d --build
```

## 2. 매일 쓰기

| 할 일 | 방법 |
|---|---|
| 켜기 | `docker compose up -d` |
| 끄기 | `docker compose down` (데이터는 남는다). **`docker compose down -v`는 DB까지 지우므로 쓰지 않는다** |
| 이 컴퓨터에서 열기 | http://127.0.0.1:3000 |
| 휴대폰에서 열기 | 아래 "휴대폰 접속" |
| 상태 보기 | `docker compose ps`, 로그는 `docker compose logs -f worker web tunnel` |

### 휴대폰 접속

1. 이 컴퓨터에서 http://127.0.0.1:3000/status 를 열면 "외부 접속(임시 주소)"에 `https://…trycloudflare.com` 주소가 보인다. 또는 `docker compose logs tunnel | grep trycloudflare`.
2. 휴대폰 브라우저에 그 주소를 넣고 `APP_PASSWORD`로 로그인한다(30일 유지).
3. 주소는 Docker를 다시 켤 때마다(노트북을 다시 켰을 때 포함) 바뀐다. 바뀌면 1번부터 다시.

알아 둘 것
- Cloudflare Quick Tunnel은 계정 없이 무료이지만 공식 문서상 테스트·개발용이라 가동 보장이 없다. 주소가 안 열리면 `docker compose restart tunnel`(새 주소가 생김).
- 노트북이 꺼지거나 절전이면 휴대폰에서도 열리지 않고 수집도 멈춘다. 다시 켜지면 놓친 수집·키워드·백업을 바로 한 번 한다.
- 같은 접속자가 비밀번호를 5번 틀리면 15분 동안 로그인을 받지 않는다. 모든 기기의 로그인을 풀려면 `APP_PASSWORD`나 `SESSION_SECRET`을 바꾸고 `docker compose up -d`.

### 화면

| 메뉴 | 내용 |
|---|---|
| 타임라인(홈) | 관련도 기준(기본 6점) 이상 기사, 최신순. 종목·영향(호재·악재·중립) 필터, "더 보기"(30개씩). 기사 하나에 카드 하나, 같은 제목은 "외 N곳"으로 묶음 |
| 종목 | 이름·티커로 찾아 등록·삭제(삭제는 숨김, 다시 등록하면 되살아남), 키워드 트리 보기·다시 만들기 |
| 상태 | 시스템 상태, 외부 접속 임시 주소, 마지막 백업, 수집 현황(최근 회차·출처별 결과·다음 수집 시각), "지금 수집" 버튼 |
| 설정 | 수집 간격, 키워드 깊이·재생성 주기·모델, 관련도 기준, 회차당 평가 수, 월 예산, 이번 달 AI 사용량 |

### 비용

- 실제로 돈이 드는 것은 OpenRouter(키워드·종목 찾기)와 TypeSafe(기사 평가)뿐이다. 수집은 무료다.
- 실측(2026-10-21): 종목 3개 기준 한 달 약 $0.4(평가 약 $0.35, 키워드 약 $0.05). 종목이 많거나 뉴스가 많은 종목일수록 늘어난다.
- 월 예산(기본 $5, 설정에서 바꿈)의 80%를 넘으면 모든 화면 위에 노란 알림, 닿으면 빨간 알림과 함께 AI 호출(평가·키워드·종목 찾기)을 멈춘다. 수집은 계속하고, 한도를 올리거나 다음 달이 되면 밀린 평가를 이어서 한다.

## 3. 백업과 복원

- 자동 백업: worker가 매일 한국 시간 04:00에 이 컴퓨터의 `backups/` 폴더(저장소 폴더 안)에 `ripple-날짜-시각.db`를 남기고 최근 7개만 둔다. 그 시각에 꺼져 있었으면 켜질 때 한 번. 상태 화면에 마지막 백업이 보인다.
- 백업 파일은 SQLite 파일 하나다(그대로 복사해 보관하면 된다). 노트북 고장에 대비하려면 `backups/` 폴더를 클라우드 드라이브 등에 따로 복사해 둔다. 개인 데이터라 git에는 올라가지 않는다(.gitignore).

복원(2026-10-21 시험용 Docker에서 확인한 순서). 복원하면 종목·기사·설정·사용량이 모두 그 백업 시각으로 돌아간다.

```bash
docker compose down
# ripple-20261022-040003.db 자리에 되돌릴 파일 이름을 넣는다.
docker run --rm -v ripple_ripple-data:/data -v "$PWD/backups":/backups:ro node:24-trixie-slim \
  sh -c 'cp /backups/ripple-20261022-040003.db /data/ripple.db && rm -f /data/ripple.db-wal /data/ripple.db-shm && chown 1000:1000 /data/ripple.db'
docker compose up -d
```

## 4. 업데이트

코드를 바꾼 뒤(또는 새 버전을 받은 뒤) 같은 명령이면 된다. DB 마이그레이션은 migrate 서비스가 먼저 적용하고, 데이터는 그대로 남는다. 임시 주소는 바뀐다.

```bash
docker compose up -d --build
```

## 5. 문제 해결

| 증상 | 확인할 것 |
|---|---|
| web이 계속 재시작 | `docker compose logs web`의 `[설정 오류]`(비밀번호·서명 키 누락이나 길이 부족) |
| 휴대폰에서 주소가 안 열림 | 노트북이 켜져 있는지, 주소가 바뀌지 않았는지(상태 화면), `docker compose logs tunnel`. 안 되면 `docker compose restart tunnel` |
| 기사가 안 보임 | 상태 화면의 최근 회차(실패한 소스, 평가 결과와 멈춘 이유). 평가 키가 없거나 예산을 다 썼거나, 관련도 기준(설정)이 높을 수 있다 |
| 종목 찾기가 안 됨 | 화면의 오류 문구(예산·OpenRouter 키·크레딧). 공식 목록에 없는 종목(한국 우선주·ETF, 미국 장외)은 등록할 수 없다 |
| 백업이 안 생김 | `docker compose logs worker \| grep 백업`. 리눅스에서는 `backups/` 폴더를 uid 1000이 쓸 수 있어야 한다 |
| 로그인이 막힘 | 5번 틀리면 15분 기다리거나 `docker compose restart web` |

## 6. 기능 자세히

### 종목과 연관 키워드
- 입력(삼전, 엔비디아, NVDA 등)으로 AI가 후보를 내고, 공식 종목 목록(KIND, NASDAQ Trader, 7일마다 갱신)에 있는 것만 최대 3개 보여 준다.
- 등록하면 worker가 따로 키워드 트리를 만든다(단계마다 5개, 기본 깊이 2). 한국 종목은 한국어, 미국 종목은 한국어·영어. 키워드마다 윗단계와 이어지는 이유와 경로(예: 삼성전자 → HBM 메모리 → 엔비디아)를 둔다.
- 매일 06:00(설정에서 주 1회로 바꿀 수 있음)에 다시 만든다. LLM은 오늘 뉴스를 모르므로 그 종목의 최근 7일 주요 기사 제목(최대 20개)을 함께 준다. 실패하면 이전 키워드를 둔다.
- 깊이를 바꾸면 다음 06:00에 반영된다. 바로 반영하려면 설정 화면의 "지금 다시 만들기".

### 기사 수집
- 소스: 언론사 RSS 29곳(피드 56개, `src/server/collect/feeds.ts`)과 Finnhub. 약관·robots.txt가 자동 수집이나 AI 이용을 명시적으로 막는 곳은 넣지 않았다(SPEC 11장). 증권사 리포트는 따로 모으지 않는다.
- 제목과 요약 앞 300자에 종목 이름이나 키워드의 낱말이 모두 있으면 그 종목에 건다. Finnhub 종목 뉴스는 그 종목에 바로 건다. 걸린 기사만 제목·요약(300자)·원문 링크·언론사·시각을 저장한다(본문은 저장하지 않음).
- 처음엔 최근 24시간, 이후엔 지난 회차 시작 1시간 전부터 받는다. 지난번에 실패한 소스는 그 소스가 마지막으로 성공한 회차부터, 새로 등록한 종목은 다음 회차에서 등록 24시간 전 기사까지 건다.
- 받기 전에 robots.txt를 확인하고, 같은 사이트에는 1.1초 이상 간격을 두고, User-Agent에 개인용 RSS 리더임을 밝힌다.

### 정기 작업
| 작업 | 언제 |
|---|---|
| 기사 수집·평가 | 마지막 성공 회차 + 30분(설정). 예정보다 5분 넘게 늦으면 '누락 보정'. 회차 전체가 실패하면 5분 뒤 다시. 종목이 없으면 쉰다 |
| 키워드 다시 만들기 | 매일 06:00(놓쳤으면 켜질 때 한 번) |
| 백업 | 매일 04:00(놓쳤으면 켜질 때 한 번) |
| 공식 종목 목록 | 30분마다 확인, 7일 지났을 때만 받음 |

### 기사 평가(Jev)
- 기사–종목 연결마다 관련도(상황 6단계 × 2 = 0~10)와 호재·악재·중립을 한 요청으로 묻는다. 모델은 `jev-1.13.0`(버전 고정, 바꿀 때는 품질을 다시 비교).
- 회차당 최대 200건(설정). 넘는 것은 다음 회차로, 종목 이름·직접 연결 먼저, 최근 기사부터, 최근 7일 기사만.
- 키가 없거나 맞지 않을 때, 예산을 다 썼을 때, 연달아 3번 실패했을 때는 그 회차 평가를 멈추고 다음 회차에 이어서 한다.

## 7. 개발

자세한 현황·규칙·검사 절차는 [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

```bash
pnpm install
cp .env.example .env  # 위 "처음 설치"대로 값을 넣는다
pnpm db:migrate       # ./data/ripple.db 생성·마이그레이션 적용
pnpm dev:worker       # 터미널 1: worker  http://127.0.0.1:4000
pnpm dev              # 터미널 2: web     http://localhost:3000
```

- Docker의 web도 3000번을 쓰므로 함께 띄우지 않는다(또는 Docker 쪽을 `WEB_PORT`로 바꾼다).
- 스키마 변경: `src/server/db/schema.ts`를 고친 뒤 `pnpm db:generate --name <이름>`, `pnpm db:migrate`.
- UI 부품 추가: `pnpm dlx shadcn@latest add <이름>`(shadcn/ui, Base UI 기반) 뒤 `pnpm format`.
- 환경변수는 `.env.example`. Docker에서는 compose가 컨테이너용 값을 정하고 비밀 값만 `.env`에서 필요한 컨테이너에 넘긴다(로그인 값은 web, AI·Finnhub 키는 worker, 내부 토큰은 둘 다).

### 검사·테스트

| 명령 | 내용 |
|---|---|
| `pnpm lint` / `pnpm typecheck` / `pnpm format:check` | 검사 |
| `pnpm test` | Vitest 단위·통합 테스트(실제 마이그레이션으로 만든 임시 DB) |
| `pnpm test:e2e` | Playwright(데스크톱·Pixel 7). 가짜 외부 서비스(4200)와 시험용 worker(4100)·web(3100)을 띄워 화면을 검사하고 `test-results/`에 캡처한다. 유료 API나 실제 언론사를 부르지 않는다 |

처음 쓸 때는 `pnpm exec playwright install chromium`.

### 시험용 Docker

본 Docker(프로젝트 `ripple`)에는 실제 데이터가 있으므로, Docker 화면을 시험할 때는 볼륨·포트·백업 폴더를 나눈 시험용 프로젝트로 띄운다. 터널도 함께 뜨므로 공개 주소가 생긴다는 점에 주의한다(외부에서 확인할 때는 공개된 시험 비밀번호 대신 무작위 비밀번호를 쓴다).

```bash
# 시험용 띄우기: 포트 3300, 백업 .check-backups, AI 키 비움(비용 없음), 시험 비밀번호
WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups OPENROUTER_API_KEY= TYPESAFE_API_KEY= FINNHUB_API_KEY= \
  APP_PASSWORD=e2e-password-not-secret SESSION_SECRET=e2e-session-secret-not-for-production-use \
  docker compose -p ripple-check up -d --build
E2E_BASE_URL=http://127.0.0.1:3300 pnpm test:e2e   # 가짜 서비스·시험 DB가 필요한 테스트는 건너뜀
# 내리기(시험 볼륨까지 지움)
WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups docker compose -p ripple-check down -v && rm -rf .check-backups
```

### 폴더 구조

```
src/app/            화면(App Router). (app)/ = 로그인 후 화면(page = 타임라인, status/, stocks/, settings/), login/ = 로그인
src/proxy.ts        모든 요청의 로그인 확인(1차)
src/components/     공용 부품: shadcn/ui(ui/), 화면 자동 새로 고침
src/server/         web·worker 공용 서버 코드(db, 환경변수, 시간, 상태 확인, 백업, 터널 주소, 설정)
src/server/auth/    로그인: 설정 확인, 세션 쿠키, 실패 제한, 서버 코드용 2차 확인, worker 내부 토큰
src/server/ai/llm/  생성형 LLM 인터페이스, OpenRouter 구현, 모델 목록 확인
src/server/ai/decider/ 기사 평가(Jev) 인터페이스, 질문 문구, TypeSafe 구현
src/server/budget/  AI 사용량 기록과 월 한도 검사(AI 호출은 반드시 여기를 거침), 이번 달 사용량 보고
src/server/listings/ 공식 종목 목록 받기·읽기(KIND, NASDAQ Trader)
src/server/stocks/  종목 매칭(LLM + 공식 목록 대조)·등록·삭제
src/server/keywords/ 연관 키워드: 프롬프트·트리 검증, 참고할 최근 기사 제목, 생성·저장, worker 대기열, 읽기
src/server/collect/ 기사 수집: RSS 목록·읽기, 요청 간격, robots.txt, 종목 연결, Finnhub, 회차 실행, 읽기
src/server/evaluate/ 기사 평가 흐름과 읽기
src/server/timeline/ 타임라인 읽기(필터·같은 제목 묶기·더 보기)
src/server/schedule/ 정기 작업(다음 시각 계산, 1분 루프)
src/server/db/      스키마, 연결, 마이그레이션 적용
src/worker/         worker 진입점, 내부 API, 마이그레이션 진입점
drizzle/            마이그레이션 SQL(drizzle-kit이 생성)
scripts/            worker 번들, Docker용 스크립트
tests/  e2e/        테스트 도우미·표본 파일, Playwright 테스트와 가짜 외부 서비스
docs/               개발 현황(DEVELOPMENT.md)
```

### 참고

- `AGENTS.md`는 `next dev`가 AI 코딩 에이전트를 감지하면 만들거나 고치는 파일이다(Next.js 기본 동작).
- Docker 이미지에서는 Next.js의 사용 통계 전송을 끈다.
- 시간 표시와 스케줄은 한국 시간(Asia/Seoul)이다.
