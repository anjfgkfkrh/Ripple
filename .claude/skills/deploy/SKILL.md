---
name: deploy
description: 본 Docker(사용자 실제 데이터)에 지금 작업 트리를 배포한다. 배포 전 확인 → 백업 → 다시 빌드 → 확인 → 기록. 사용자가 배포를 요청했을 때만 쓴다.
disable-model-invocation: true
---

본 Docker(compose 프로젝트 `ripple`, 볼륨 `ripple_ripple-data`)에는 사용자 실제 데이터가 있다. 이 절차 밖의 일(비밀번호·설정·종목·예산 바꾸기, `down`, `-v`, 볼륨 지우기)은 하지 않는다. 명령은 모두 저장소 루트에서, `-p` 없이 돈다.

## 1. 배포 전 확인

- `git status --short`, `git log -1 --oneline`: 빌드는 커밋이 아니라 작업 트리 기준이다. 커밋하지 않은 변경이 있으면 보고에 적는다.
- 이번 변경 뒤에 돌린 `/check` 결과가 있는지 본다. 없으면 먼저 돌린다.
- 지금 상태(읽기 전용):

```bash
docker compose ps --format 'table {{.Service}}\t{{.Status}}'
docker compose logs tunnel 2>&1 | grep -o 'https://[a-z0-9-]*\.trycloudflare\.com' | tail -1
docker compose exec -T worker sqlite3 -readonly /data/ripple.db "
select 'articles', count(*) from articles;
select 'userVersion', user_version from pragma_user_version;
select 'stocks', kind, count(*) from stocks where deleted_at is null group by kind;
select 'lastRun', id, status, started_at from collection_runs order by id desc limit 1;"
```

## 2. 백업

자동 백업과 같은 이름(`ripple-YYYYMMDD-HHMMSS.db`, 한국 시간)이라 다음 04:00 정리 때 오래된 것부터 지워진다.

```bash
TS="$(TZ=Asia/Seoul date +%Y%m%d-%H%M%S)"
docker compose exec -T worker sqlite3 -readonly /data/ripple.db ".backup /backups/ripple-$TS.db"
docker compose exec -T worker sqlite3 -readonly "/backups/ripple-$TS.db" "select 'articles', count(*) from articles;"
```

백업의 기사 수가 1의 수와 다르면(수집이 그 사이 돌았으면 조금 늘 수 있다) 이유를 확인한 뒤 진행한다.

## 3. 배포

```bash
docker compose up -d --build
```

migrate·worker·web만 다시 만들어진다. tunnel은 이미지와 설정이 그대로라 다시 만들어지지 않고 주소도 유지된다.

## 4. 확인

- `docker compose ps`: worker `healthy`, web `Up`, tunnel은 배포 전과 같은 가동 시간(다시 만들어지지 않음).
- `docker compose logs migrate --tail 20`: 오류 없음. 1의 조회를 다시 돌려 `userVersion`이 `rust/ripple/migrations/*.sql` 파일 수와 같고(지금 11), 기사 수가 배포 전 이상인지 본다.
- 미로그인 차단: `curl -s -o /dev/null -w '%{http_code} %{redirect_url}\n' http://127.0.0.1:3000/` → `307 …/login`
- 터널 주소: 1과 같은지.
- `docker compose logs --since 10m worker web 2>&1 | grep -i -E 'error|fail' | head`: 새 오류 없음.
- 이번 변경에 맞는 확인을 하나 더 한다(예: 새 API를 데이터가 바뀌지 않는 방식으로 부르기). 데이터를 바꾸는 확인은 하지 않는다.
- AI 호출은 하지 않는다(정기 작업이 부르는 것은 평소대로).

## 5. 기록

- DEVELOPMENT.md 1장(지금 상태)과 2장(단계 표 "배포" 날짜), DECISIONS.md 3장 맨 위에 배포 기록: 시각, HEAD 커밋과 커밋하지 않은 변경 여부, 백업 파일·기사 수, 마이그레이션 수, 확인 결과.
- 사용자에게 같은 내용을 표로 보고한다. 휴대폰·PC 화면 확인은 사용자 몫이라고 적는다.
