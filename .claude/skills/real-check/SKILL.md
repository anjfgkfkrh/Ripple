---
name: real-check
description: 시험용 Docker(compose 프로젝트 ripple-check)에서 실제 환경 확인을 한다. 본 Docker를 건드리지 않고 마이그레이션·화면·백업·실제 데이터·실제 AI 흐름을 확인할 때 쓴다. 실제 AI 확인은 비용 승인을 받은 뒤에만.
argument-hint: "[무료|AI]"
---

본 Docker(프로젝트 `ripple`)는 사용자 실제 데이터라 쓰지 않는다. docker-compose.yml에 `name: ripple`이 박혀 있어서, compose 명령에 아래 변수와 `-p ripple-check`를 빠뜨리면 본 Docker가 바뀐다. 모든 compose 명령을 이렇게 시작한다:

```bash
WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups docker compose -p ripple-check …
```

| 종류 | AI 키 | 터널 | 비용 |
|---|---|---|---|
| 무료(기본) | 비움 | 띄우지 않음 | 0원 |
| AI | `.env`의 실제 키 | 띄우지 않음 | 있음. 예상 비용을 말하고 사용자 승인을 받은 뒤에만 |

## 1. 빌드

```bash
WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups docker compose -p ripple-check build
```

## 2. 실제 데이터 사본 넣기(필요할 때)

본 DB를 직접 쓰지 않고 백업 사본을 시험 볼륨에 넣는다. 최신 백업은 `ls -t backups/ | head -1`(매일 04:00 자동, 7개 보관).

```bash
WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups docker compose -p ripple-check run --rm --no-deps \
  -v "$PWD/backups":/restore:ro worker sh -c 'cp /restore/<백업 파일> /data/ripple.db'
```

- 켜자마자 정기 작업(06:00 키워드 재생성 등)이 돌면 확인이 흐려진다. 막아야 하면 같은 방식(`run --rm --no-deps worker node -e …`)으로 시험 DB의 settings 값을 바꾼다. 키 이름과 값 형식은 `src/server/settings.ts`에서 확인한다. 키워드 재생성은 `keywords.autoRegeneratedAt`을 지금 시각으로 두면 이번 주기에 이미 한 것으로 본다(`src/server/schedule/plan.ts`).

## 3. 띄우기

무료(AI 키를 비워 비용이 생기지 않게):

```bash
WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups OPENROUTER_API_KEY= TYPESAFE_API_KEY= FINNHUB_API_KEY= \
  APP_PASSWORD=e2e-password-not-secret SESSION_SECRET=e2e-session-secret-not-for-production-use \
  docker compose -p ripple-check up -d migrate worker web
```

- AI: 키 변수 세 개를 빼고 띄운다(`.env`의 실제 키가 쓰인다). 비용 승인 먼저.
- 위 비밀번호는 README에 공개된 시험 값이라 터널 없이 127.0.0.1:3300에서만 쓴다. 휴대폰 확인처럼 공개 주소가 필요하면 `tunnel`도 띄우고, 무작위 비밀번호를 파일로 만들어 쓰고(출력하지 않음) 끝나면 지운다. 주소는 `… logs tunnel | grep trycloudflare`.

## 4. 확인

- 화면: http://127.0.0.1:3300 (캡처는 Playwright로 찍어 직접 본다. 시험용 Docker에 E2E를 돌릴 때는 `E2E_BASE_URL=http://127.0.0.1:3300 pnpm test:e2e`, 가짜 서비스가 필요한 시험은 건너뛴다).
- 데이터(읽기 전용): `… exec -T worker node -e '…'`로 better-sqlite3를 `{ readonly: true }`로 연다(`/deploy` 1장의 조회와 같은 식). 긴 스크립트는 표준 입력으로 넘긴다(`node - < 스크립트.cjs`, 컨테이너 밖 `/tmp`에 두면 패키지를 못 찾는다).
- worker 내부 API는 컨테이너 안에서 부른다. 토큰을 밖으로 꺼내지 않는다:
  `fetch("http://127.0.0.1:4000/<경로>", { method: "POST", headers: { authorization: "Bearer " + process.env.WORKER_API_TOKEN } })`
- 지난 기사까지 보려면 시험 DB의 `stocks.backfill_from`을 24시간 전으로 두고 `POST /collect`.

## 5. AI 확인이었으면 사용량 옮기기

시험 DB의 이번 `usage_logs`를 용도(`purpose`) 끝에 `_check`를 붙여 본 DB에 넣는다(월 합계를 맞추려고, 본 DB에 쓰는 것은 이것뿐). 쓰기 연결, `busy_timeout` 5초, 한 트랜잭션. 넣기 전에 몇 줄·얼마인지 보고 넣은 뒤 본 DB 합계를 다시 본다. 열 이름은 `src/server/db/schema.ts`의 `usageLogs`.

## 6. 내리기

```bash
WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups docker compose -p ripple-check down -v
rm -rf .check-backups
docker image rm ripple-check-web ripple-check-worker ripple-check-migrate
```

`docker compose ls`로 `ripple-check`가 없어졌고 `ripple`은 그대로인지 본다.

## 7. 기록

결과는 DECISIONS.md 2장(실측 기록), 비용은 1장 "N단계 실제 API 호출" 줄과 DEVELOPMENT.md 5장에 적는다.
