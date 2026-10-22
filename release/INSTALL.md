# Ripple __VERSION__ 설치 안내

보유 주식 관련 뉴스를 자동으로 모으고 AI(Jev)로 관련도와 호재·악재를 골라 웹으로 보여 주는 개인용 앱입니다. 이 묶음은 Docker 이미지를 미리 만들어 둔 것이라 따로 빌드하지 않고 실행합니다.

## 준비물

- Docker Desktop(Mac·Windows) 또는 Docker Engine + compose v2(Linux)
- API 키: OpenRouter(필수), TypeSafe(필수), Finnhub(선택). 비용은 각자 부담합니다(종목 3개 기준 한 달 약 $0.4).

## 받을 파일(릴리스 페이지에서)

| 파일 | 설명 |
|---|---|
| `ripple-__VERSION__-images-arm64.tar.gz` **또는** `ripple-__VERSION__-images-amd64.tar.gz` | 내 컴퓨터에 맞는 **하나만**. Apple Silicon Mac(M1 이후)은 arm64, Intel·AMD PC(Windows, Linux, Intel Mac)는 amd64. 터미널에서 `uname -m`이 `arm64`·`aarch64`면 arm64, `x86_64`면 amd64 |
| `docker-compose.yml` | 실행 설정 |
| `env.example` | 환경변수 예시(이 이름을 `.env`로 복사해 씁니다) |
| `SHA256SUMS` | (선택) 받은 파일이 온전한지 확인용 |

모두 같은 폴더에 둡니다. 이 폴더가 앞으로 앱을 켜고 끄는 폴더입니다.

## 설치

```bash
# 1. (선택) 받은 파일 확인
shasum -a 256 -c SHA256SUMS --ignore-missing

# 2. 이미지 불러오기(몇 분 걸릴 수 있음). 파일 이름은 내가 받은 것으로
docker load < ripple-__VERSION__-images-arm64.tar.gz

# 3. 설정 파일 만들기
cp env.example .env
```

`.env`를 열어 값을 채웁니다.

| 이름 | 필요 | 내용 |
|---|---|---|
| `APP_PASSWORD` | 필수 | 로그인 비밀번호(7자 이상). 공개 주소로 열리므로 짐작하기 어렵게 |
| `SESSION_SECRET` | 필수 | 32자 이상 무작위 값: `openssl rand -base64 48` |
| `WORKER_API_TOKEN` | 필수 | 32자 이상 무작위 값: `openssl rand -base64 48` |
| `OPENROUTER_API_KEY` | 필수 | https://openrouter.ai 에서 발급. 키에 사용 한도를 걸어 두길 권장 |
| `TYPESAFE_API_KEY` | 필수 | https://console.typesafe.ai 에서 발급(기사 평가). 없으면 기사는 모으고 평가는 미룸 |
| `FINNHUB_API_KEY` | 선택 | https://finnhub.io 에서 무료 가입(미국 종목 뉴스) |

```bash
# 4. 켜기
docker compose up -d
```

## 사용

- 이 컴퓨터에서: http://127.0.0.1:3000 → `APP_PASSWORD`로 로그인 → 메뉴 '종목'에서 종목 등록(예: 삼전, NVDA).
- 휴대폰에서: http://127.0.0.1:3000/status 의 "외부 접속(임시 주소)"에 나온 `https://…trycloudflare.com` 주소를 휴대폰 브라우저에 넣습니다. 또는 `docker compose logs tunnel | grep trycloudflare`. 주소는 다시 켤 때마다 바뀌고, 가동 보장이 없는 무료 임시 주소입니다.
- 정기 수집은 30분마다, 키워드 다시 만들기는 매일 06:00, 백업은 매일 04:00(이 폴더의 `backups/`, 최근 7개)입니다. 노트북이 꺼져 있으면 멈추고, 켜지면 놓친 것을 바로 한 번 합니다.
- 설정(수집 간격, 월 예산 한도 등)은 웹의 '설정' 메뉴에서 바꿉니다. 월 예산 기본값은 $5이고, 넘으면 AI 호출만 멈춥니다.

## 켜고 끄기·상태

| 할 일 | 명령 |
|---|---|
| 끄기 | `docker compose down` (데이터는 남음. **`down -v`는 DB까지 지우므로 쓰지 않기**) |
| 켜기 | `docker compose up -d` |
| 상태 | `docker compose ps`, 로그 `docker compose logs -f worker web tunnel` |

## 업데이트

새 버전의 이미지 파일과 `docker-compose.yml`을 같은 폴더에 받아 덮어쓴 뒤:

```bash
docker load < ripple-새버전-images-arm64.tar.gz
docker compose up -d
```

데이터(DB)는 그대로 남고, 필요한 DB 변경은 자동으로 적용됩니다.

## 백업 복원

`backups/` 폴더의 `ripple-날짜-시각.db`로 되돌립니다(종목·기사·설정이 그 시각으로 돌아감).

```bash
docker compose down
docker run --rm -v ripple_ripple-data:/data -v "$PWD/backups":/backups:ro ripple-worker:__VERSION__ \
  sh -c 'cp /backups/되돌릴파일.db /data/ripple.db && rm -f /data/ripple.db-wal /data/ripple.db-shm'
docker compose up -d
```

## 문제가 생기면

| 증상 | 확인 |
|---|---|
| web이 계속 재시작 | `docker compose logs web`의 `[설정 오류]`(비밀번호·키 길이 부족) |
| `no matching manifest`·`exec format error` | 내 컴퓨터와 다른 종류(arm64/amd64) 파일을 불러온 것입니다. 맞는 파일로 다시 `docker load` |
| 휴대폰에서 주소가 안 열림 | 컴퓨터가 켜져 있는지, 주소가 바뀌지 않았는지(상태 화면), 안 되면 `docker compose restart tunnel` |
| 기사가 안 보임 | 웹 '상태' 화면의 최근 수집 결과, 평가 키, 월 예산, '설정'의 관련도 기준 |

소스와 자세한 설명: https://github.com/anjfgkfkrh/Ripple
