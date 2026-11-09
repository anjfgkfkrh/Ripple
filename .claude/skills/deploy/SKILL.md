---
name: deploy
description: 본 Docker(원격 서버 blockall-ss, 사용자 실제 데이터)에 지금 작업 트리를 배포한다. 배포 전 확인 → 백업 → Mac에서 amd64 빌드·전송 → 다시 띄우기 → 확인 → 기록. 사용자가 배포를 요청했을 때만 쓴다.
disable-model-invocation: true
---

본 Docker(compose 프로젝트 `ripple`, 볼륨 `ripple_ripple-data`)는 2026-11-09부터 원격 서버 `blockall@blockall-ss`(Ubuntu x86_64)의 `~/Ripple`에서 돈다(DECISIONS.md 1장 "본 Docker 원격 서버로 옮기기"). 원격에는 `docker-compose.yml`·`.env`·`backups/`만 있고 소스는 없다. 이미지는 Mac에서 amd64로 빌드해 보낸다.

이 절차 밖의 일(비밀번호·설정·종목·예산 바꾸기, `down`, `-v`, 볼륨 지우기)은 하지 않는다. **Mac에서는 `-p ripple-check` 없이 `docker compose up`하지 않는다**: 같은 터널 토큰과 worker가 원격과 겹친다. Mac의 `ripple` 컨테이너·볼륨은 2026-11-09에 지웠다. 로컬 명령은 저장소 루트에서 돈다. 원격 명령은 `R='ssh blockall@blockall-ss'`로 두고 `$R 'cd ~/Ripple && …'`로 부른다.

## 1. 배포 전 확인

- `git branch --show-current`: main이어야 한다. agentflow 단계 브랜치(`af/<단계>`)이면 멈추고, 사용자에게 main에 합칠지 묻는다(합치기 전 `node scripts/check-secrets.mjs`).
- `git status --short`, `git log -1 --oneline`: 빌드는 커밋이 아니라 작업 트리 기준이다. 커밋하지 않은 변경이 있으면 보고에 적는다.
- 이번 변경 뒤에 돌린 `/check` 결과가 있는지 본다. 없으면 먼저 돌린다.
- `docker-compose.yml`이 원격 것과 다르면(`diff <($R 'cat ~/Ripple/docker-compose.yml') docker-compose.yml`) 3장에서 함께 보낸다. 새 환경 변수가 생겼으면 원격 `.env`에 넣어야 하므로 사용자에게 알린다(`.env`는 읽지 않는다).
- 지금 상태(읽기 전용):

```bash
$R 'cd ~/Ripple && docker compose ps --format "table {{.Service}}\t{{.Status}}"
docker compose exec -T web sh -c "curl -s http://tunnel:20241/ready"; echo
docker compose exec -T worker sqlite3 -readonly /data/ripple.db "
select \"articles\", count(*) from articles;
select \"userVersion\", user_version from pragma_user_version;
select \"stocks\", kind, count(*) from stocks where deleted_at is null group by kind;
select \"lastRun\", id, status, started_at from collection_runs order by id desc limit 1;"'
```

## 2. 백업

자동 백업과 같은 이름(`ripple-YYYYMMDD-HHMMSS.db`, 한국 시간)이라 다음 04:00 정리 때 오래된 것부터 지워진다. 원격 `~/Ripple/backups`에 남는다.

```bash
TS="$(TZ=Asia/Seoul date +%Y%m%d-%H%M%S)"
$R "cd ~/Ripple && docker compose exec -T worker sqlite3 -readonly /data/ripple.db '.backup /backups/ripple-$TS.db' && docker compose exec -T worker sqlite3 -readonly /backups/ripple-$TS.db 'select count(*) from articles;'"
```

백업의 기사 수가 1의 수와 다르면(수집이 그 사이 돌았으면 조금 늘 수 있다) 이유를 확인한 뒤 진행한다.

## 3. 빌드·전송·배포

```bash
docker buildx build --platform linux/amd64 --target ripple --load -t ripple-app:amd64 .
docker save ripple-app:amd64 | gzip | $R 'gunzip | docker load && docker tag ripple-app:amd64 ripple-app:latest'
# compose 파일이 바뀌었을 때만:
scp -q docker-compose.yml blockall@blockall-ss:Ripple/docker-compose.yml
$R 'cd ~/Ripple && docker compose config --quiet && docker compose up -d --no-build'
```

- Mac(arm64)에서 QEMU로 빌드한다(`scripts/release.sh`와 같은 방식). `--no-build`는 원격에 소스가 없어서 꼭 붙인다.
- `-t ripple-app:amd64`로 따로 이름을 붙여 Mac의 시험용 이미지(`ripple-app`, arm64)를 덮지 않는다.
- `config`는 비밀 값을 출력하므로 `--quiet`로만 쓴다.
- migrate·worker·web만 다시 만들어진다. tunnel은 이미지와 설정이 그대로라 다시 만들어지지 않는다(tunnel 서비스의 명령·환경이 바뀐 배포는 예외).

## 4. 확인

- `$R 'cd ~/Ripple && docker compose ps'`: worker `healthy`, web `Up`, tunnel은 배포 전과 같은 가동 시간(tunnel 설정이 바뀐 배포는 예외).
- `docker compose logs migrate --tail 20`(원격): 오류 없음. 1의 조회를 다시 돌려 `userVersion`이 `rust/ripple/migrations/*.sql` 파일 수와 같고(지금 11), 기사 수가 배포 전 이상인지 본다.
- 원격에서 `curl -s -o /dev/null -w '%{http_code} %{redirect_url}\n' http://127.0.0.1:3000/` → `200`(손님 보기), `/status` → `307 …/login?next=%2Fstatus`.
- 터널: 1의 `/ready`가 `200`·`readyConnections` 4. 공개 주소는 tunnel 로그 `ingress`의 `hostname`(지금 `ripple.anjfgkfkrh.com`)으로 Mac에서 `curl`한다.
- `docker compose logs --since 10m worker web 2>&1 | grep -i -E 'error|fail' | head`(원격): 새 오류 없음. tunnel의 `failed to sufficiently increase receive buffer size`(UDP 버퍼) 경고는 옮길 때부터 있던 것이다.
- 이번 변경에 맞는 확인을 하나 더 한다(예: 새 API를 데이터가 바뀌지 않는 방식으로 부르기). 데이터를 바꾸는 확인은 하지 않는다.
- AI 호출은 하지 않는다(정기 작업이 부르는 것은 평소대로).

## 5. 기록

- STATE.md(지금 상태)와 DEVELOPMENT.md 2장(단계 표 "배포" 날짜), DECISIONS.md 3장 맨 위에 배포 기록: 시각, HEAD 커밋과 커밋하지 않은 변경 여부, 백업 파일·기사 수, 마이그레이션 수, 확인 결과.
- 사용자에게 같은 내용을 표로 보고한다. 휴대폰·PC 화면 확인은 사용자 몫이라고 적는다.

## 6. 되돌리기(배포가 망가졌을 때만, 사용자 허락 뒤)

- 코드만 문제면: 이전 커밋으로 3장을 다시 한다.
- DB가 문제면: 원격 worker·web을 `stop`하고, 2장 백업을 임시 컨테이너로 볼륨에 넣은 뒤(`docker run --rm -v ripple_ripple-data:/data -v ~/Ripple/backups:/backups ripple-app cp /backups/<파일> /data/ripple.db`, `-wal`·`-shm`은 지운다) `up -d --no-build`.
