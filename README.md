# Ripple

보유 주식 관련 뉴스·증권사 리포트를 자동 수집하고 Jev로 선별해 웹으로 보여주는 개인용 MVP.
요구사항·결정 기록·미결정 목록은 [SPEC.md](SPEC.md)에 있다.

> 현재 4단계(기사 수집)까지 구현했다. 홈 화면은 아직 시스템 상태·수집 확인용이다.

## 구성

| 서비스 | 역할 |
|---|---|
| `migrate` | 일회성. 아직 적용하지 않은 마이그레이션을 적용하고 종료한다(worker 이미지 사용) |
| `worker` | DB에 쓰는 유일한 프로세스. 내부 API(포트 4000, compose 네트워크 안에서만, 공유 토큰 필요)로 web의 쓰기·AI 요청을 받는다. 공식 종목 목록을 주 1회 받고, 종목의 연관 키워드를 만들고, 기사를 수집한다. 이후 단계에서 평가·스케줄을 맡는다 |
| `web` | Next.js 화면. 로그인한 사용자만 쓸 수 있다. DB는 읽기 전용으로 열고, 쓰기는 worker 내부 API에 요청한다 (SPEC D-36) |

DB는 SQLite(Drizzle ORM, better-sqlite3)이고, 파일은 named volume `ripple_ripple-data`의 `/data/ripple.db`에 있으며 WAL 모드로 동작한다.

## 준비물

- Node.js 24, pnpm 12 (`brew install pnpm`)
- Docker Desktop (compose v2)

## 처음 설정: 로그인 비밀번호

`.env`에 로그인 비밀번호와 세션 서명 키를 넣는다. 둘 다 없거나 짧으면 web이 시작하지 않는다.

```bash
cp .env.example .env              # 아직 .env가 없을 때만
# .env를 열어 APP_PASSWORD=에 7자 이상 비밀번호를 적는다.
# SESSION_SECRET이 비어 있으면 32자 이상 무작위 값을 넣는다: openssl rand -base64 48
```

- 로그인 유지 기간은 30일이다(SPEC D-10).
- 같은 IP에서 5번 틀리면 15분 동안 로그인을 받지 않는다. 기록은 web 메모리에 있어 web을 재시작하면 풀린다.
- 모든 기기의 로그인을 한 번에 풀려면 `APP_PASSWORD`나 `SESSION_SECRET`을 바꾸고 web을 다시 띄운다.
- `WORKER_API_TOKEN`(web↔worker 공유 토큰)도 32자 이상 필요하다. 비어 있으면 같은 방법으로 만든다.

## 종목 등록 (2단계)

- `/stocks` 화면에서 종목명·줄임말·티커(예: 삼전, 엔비디아, NVDA)를 입력하면 LLM이 후보를 내고, 공식 종목 목록에 있는 것만 최대 3개 보여 준다(SPEC D-12).
- 공식 목록: 한국은 KIND 상장법인목록, 미국은 NASDAQ Trader 종목 목록. worker가 시작할 때와 이후 7일마다 받는다. 한국 우선주·ETF와 미국 장외(OTC) 종목은 목록에 없어 등록할 수 없다.
- 삭제하면 숨기기만 한다. 다시 등록하면 되살아난다(D-11).
- LLM 호출에는 `.env`의 `OPENROUTER_API_KEY`가 필요하다(worker에만 전달). OpenRouter 화면에서 키에 사용 한도를 걸어 두는 것을 권장한다.
- AI 비용 안전장치: 호출마다 OpenRouter가 알려 준 실제 청구액을 기록하고, 이번 달(한국 시간) 합계가 한도(임시 $5, 8단계에서 다시 정함)에 닿으면 AI 호출을 막는다.
- 기본 모델은 `anthropic/claude-haiku-5.5`(D-02). 설정 화면(8단계) 전까지는 settings 테이블의 `llm.model` 값으로 바꿀 수 있다.

## 연관 키워드 (3단계)

- 종목을 등록하면(또는 지운 종목을 다시 등록하면) worker가 따로 LLM을 한 번 불러 연관 키워드 트리를 만든다. 등록은 기다리지 않고 바로 끝난다(D-37).
- 단계마다 5개, 기본 깊이 2(10개). 2단계부터는 바로 윗단계 키워드 하나에 이어진다(D-13). 깊이는 설정 화면(8단계) 전까지 settings 테이블의 `keywords.depth`(1~4)로 바꾼다.
- 한국 종목은 한국어, 미국 종목은 한국어와 영어 표기를 함께 만든다(D-14). 키워드마다 윗단계와 이어지는 이유 한 줄과 경로(예: 삼성전자 → HBM 메모리 → 엔비디아)를 저장한다.
- `/stocks` 목록에서 "키워드 N개"를 누르면 트리가 펼쳐진다. 만드는 동안에는 "키워드를 만드는 중…"이 보이고, 화면이 3초마다 스스로 다시 읽는다.
- 실패하면 이유와 "키워드 다시 만들기" 버튼이 보인다. 이전 키워드는 그대로 남는다. 다시 만들면 통째로 바뀐다(수동 편집 없음, D-15).
- worker를 다시 시작하면, 기다리던 종목과 만들다 멈춘 종목을 이어서 만든다.
- 비용(2026-10-20 실측): 트리 1개에 깊이 2는 약 $0.0005·5초, 깊이 4는 약 $0.0009·9초. worker 로그에 종목마다 걸린 시간과, 규칙에 맞지 않아 버린 키워드 수가 남는다.
- 하루 1회 자동으로 다시 만드는 기능은 6단계에서 붙인다.

## 기사 수집 (4단계)

- 소스: 언론사 RSS 29곳(피드 56개, 목록은 `src/server/collect/feeds.ts`)과 Finnhub(미국 종목 뉴스·시장 뉴스). 약관·robots.txt가 자동 수집이나 AI 이용을 명시적으로 막는 곳은 넣지 않았다(SPEC 11장). 증권사 리포트는 따로 모으지 않고, 리포트를 다룬 뉴스로 대신한다.
- 실행: 상태 화면(홈)의 "지금 수집" 버튼. 정기 수집은 6단계에서 붙인다. 수집 중에는 화면이 3초마다 스스로 다시 읽고, 끝나면 새 기사·연결 수와 출처별 결과(받음·새 기사·연결·실패)를 보여 준다.
- 종목에 거는 방법: 제목+요약(앞 300자)에 종목 이름이나 키워드의 낱말이 모두 있으면 건다. 종목마다 가장 가까운 경로 하나(종목 이름 > 1단계 > 2단계 …)를 기사와 함께 글자로 남긴다. Finnhub 종목 뉴스는 그 종목에 바로 걸고, 시장 뉴스는 영어 키워드로 건다.
- 저장: 종목에 걸린 기사만, 제목·요약(300자)·원문 링크·언론사·발행 시각. 본문은 저장하지 않는다. 원문 주소가 같으면 한 기사로 합친다.
- 기간: 처음엔 최근 24시간, 이후엔 지난번 수집 시작 1시간 전부터(최대 7일). RSS는 피드에 든 만큼만 받으므로(피드마다 약 2시간~며칠치) 수집 간격이 길면 놓칠 수 있다.
- 예의: 받기 전에 robots.txt를 확인하고(하루 동안 기억), 같은 사이트에는 1.1초 이상 간격을 두고, User-Agent에 개인용 RSS 리더임을 밝힌다.
- 미국 뉴스에는 `.env`의 `FINNHUB_API_KEY`가 필요하다(무료 가입, worker에만 전달). 없으면 미국 뉴스는 건너뛴다.
- 실측(2026-10-20, 종목 3개: 삼성전자·엔비디아·현대건설): 한 번에 약 24초, 받은 기사 4,417건 중 최근 24시간 2,622건, 저장 313건(연결 348건). 그중 Finnhub 엔비디아 종목 뉴스가 154건.

## Docker로 실행

```bash
docker compose up -d --build
```

- 접속: http://127.0.0.1:3000 — 이 컴퓨터에서만 열린다(외부 공개는 9단계 Cloudflare Tunnel).
- 상태·로그: `docker compose ps`, `docker compose logs -f worker web`
- web이 계속 재시작되면 `docker compose logs web`에서 `[설정 오류]`를 확인한다(비밀번호·서명 키 누락 등).
- 중지: `docker compose down` (데이터는 볼륨에 남는다). `docker compose down -v`는 **DB까지 지운다.**
- DB 파일 확인: `docker run --rm -v ripple_ripple-data:/data node:24-trixie-slim ls -la /data`
- 코드를 바꾼 뒤에도 같은 명령(`docker compose up -d --build`)이면 된다. 새 마이그레이션은 migrate가 먼저 적용한다.

## 로컬 개발

```bash
pnpm install
cp .env.example .env  # 그다음 위 "처음 설정"대로 비밀번호를 넣는다
pnpm db:migrate       # ./data/ripple.db 생성·마이그레이션 적용
pnpm dev:worker       # 터미널 1: worker  http://127.0.0.1:4000
pnpm dev              # 터미널 2: web     http://localhost:3000
```

- 스키마 변경: `src/server/db/schema.ts`를 고친 뒤 `pnpm db:generate --name <이름>`으로 `drizzle/`에 마이그레이션을 만들고 `pnpm db:migrate`로 적용한다.
- UI 부품 추가: `pnpm dlx shadcn@latest add <이름>` (shadcn/ui, Base UI 기반). 추가한 뒤 `pnpm format`.
- Docker의 web도 3000번을 쓰므로 둘을 동시에 띄우지 않는다.

### 환경변수

`.env.example` 참고. 지금 쓰는 값은 `DATABASE_PATH`, `WORKER_URL`, `WORKER_HOST`, `WORKER_PORT`, `TZ`, `APP_PASSWORD`, `SESSION_SECRET`, `WORKER_API_TOKEN`, `OPENROUTER_API_KEY`, `FINNHUB_API_KEY`다.
Docker에서는 compose 파일이 컨테이너용 값을 정하고, 비밀 값만 `.env`에서 읽어 필요한 곳에만 넘긴다(로그인 값은 web, OpenRouter·Finnhub 키는 worker, 공유 토큰은 둘 다). `.env`는 커밋하지 않는다.

## 검사·테스트

| 명령 | 내용 |
|---|---|
| `pnpm lint` | ESLint |
| `pnpm typecheck` | 타입 검사 (Next 라우트 타입 생성 포함) |
| `pnpm format:check` / `pnpm format` | Prettier 검사 / 적용 |
| `pnpm test` | Vitest 단위·통합 테스트. 실제 마이그레이션으로 만든 임시 DB를 쓴다 |
| `pnpm test:e2e` | Playwright. 가짜 외부 서비스(4200: 공식 목록 표본, 종목 찾기·키워드 생성을 흉내 내는 OpenRouter, 언론사 RSS 2개, Finnhub)와 테스트용 worker(4100)·web(3100)을 띄워 화면을 검사하고 `test-results/`에 캡처한다. 유료 API나 실제 언론사를 부르지 않는다 |
| Docker 화면 검사 | 아래 참고 |

Docker로 띄운 화면을 검사하려면 테스트 전용 비밀번호로 띄운 뒤 검사하고, 끝나면 내린다(테스트 값은 `e2e/test-env.ts`에 공개돼 있으므로 그대로 두지 않는다). 종목·키워드·수집 E2E는 가짜 외부 서비스가 필요해 이 방식에서는 건너뛴다.

```bash
APP_PASSWORD=e2e-password-not-secret SESSION_SECRET=e2e-session-secret-not-for-production-use docker compose up -d --build
E2E_BASE_URL=http://127.0.0.1:3000 pnpm test:e2e
docker compose down
```

Playwright를 처음 쓸 때는 `pnpm exec playwright install chromium`으로 브라우저를 받는다.

## 폴더 구조

```
src/app/            Next.js 화면 (App Router). (app)/ 아래는 로그인 후 화면, login/은 로그인 화면
src/proxy.ts        모든 요청의 로그인 확인(1차)
src/components/     공용 부품: shadcn/ui(ui/), 화면 자동 새로 고침
src/server/         web·worker 공용 서버 코드 (db, 환경변수, 시간, 상태 확인)
src/server/auth/    로그인: 설정 확인, 세션 쿠키, 실패 제한, 서버 코드용 2차 확인, worker 공유 토큰
src/server/ai/llm/  생성형 LLM 인터페이스와 OpenRouter 구현
src/server/budget/  AI 사용량 기록과 월 한도 검사(AI 호출은 반드시 여기를 거침)
src/server/listings/ 공식 종목 목록 받기·읽기(KIND, NASDAQ Trader)
src/server/stocks/  종목 매칭(LLM + 공식 목록 대조)·등록·삭제
src/server/keywords/ 연관 키워드: 프롬프트와 트리 검증(tree), 생성·저장(generate), worker 대기열(queue), 읽기(read)
src/server/collect/ 기사 수집: RSS 목록(feeds), RSS 읽기(rss), 요청 간격(http), robots.txt(robots), 종목 연결(match), Finnhub(finnhub), 회차 실행(run·runner), 읽기(read)
src/server/db/      스키마(schema.ts), 연결, 마이그레이션 적용
src/worker/         worker 진입점, 내부 API, 마이그레이션 진입점
drizzle/            마이그레이션 SQL (drizzle-kit이 생성)
scripts/            worker 번들, Docker용 스크립트
tests/  e2e/        테스트 도우미·표본 파일(tests/fixtures), Playwright 테스트와 가짜 외부 서비스
```

## 참고

- `AGENTS.md`는 `next dev`가 AI 코딩 에이전트를 감지하면 만들거나 고치는 파일이다(Next.js 기본 동작).
- Docker 이미지에서는 Next.js의 사용 통계 전송을 끈다.
- 시간 표시는 한국 시간(Asia/Seoul)이다.
