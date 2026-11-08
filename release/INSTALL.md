# Ripple __VERSION__ 설치 안내

보유 주식 관련 뉴스를 자동으로 모으고 AI(Jev)로 관련도와 호재·악재를 골라 웹으로 보여 주는 개인용 앱입니다. 이 묶음은 Docker 이미지를 미리 만들어 둔 것이라 따로 빌드하지 않고 실행합니다.

## 준비물

- Docker Desktop(Mac·Windows) 또는 Docker Engine + compose v2(Linux)
- API 키: OpenRouter(필수), TypeSafe(필수), Finnhub(선택). 비용은 각자 부담합니다(종목 3개 기준 한 달 약 $3).

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
| `FINNHUB_API_KEY` | 선택 | https://finnhub.io 에서 무료 가입(미국 종목 뉴스·주가) |
| `KIS_APP_KEY`, `KIS_APP_SECRET` | 선택 | 한국투자증권 KIS Open API 앱 키·시크릿(https://apiportal.koreainvestment.com, 한국 종목 주가). 없으면 한국 종목 주가는 보이지 않음 |
| `KIS_ENV` | 선택 | `demo`(모의투자, 기본) 또는 `real`. 키가 새어도 실제 계좌로 주문되지 않게 모의투자 키를 권장 |
| `TUNNEL_TOKEN`, `PUBLIC_URL` | 선택 | 고정 주소용 Cloudflare 터널 토큰(비밀 값)과 그 주소. 비우면 임시 주소. 아래 "선택: 고정 도메인" |
| `GUEST_VIEW` | 선택 | 로그인 안 한 사람(손님)에게 타임라인·종목 보기를 열어 둘지. 비우거나 `on`이면 열림, `off`면 끔. 아래 "손님 보기" |
| `EDIT_UNLOCK_MINUTES` | 선택 | 비밀번호를 넣은 뒤 수정할 수 있는 시간(분). 1~120, 비우면 30 |

```bash
# 4. 켜기
docker compose up -d
```

## 사용

- 이 컴퓨터에서: http://127.0.0.1:3000 → `APP_PASSWORD`로 로그인 → 메뉴 '종목'에서 종목 등록(예: 삼전, NVDA).
- 휴대폰에서(토큰이 없을 때): http://127.0.0.1:3000/status 의 "외부 접속(임시 주소)"에 나온 `https://…trycloudflare.com` 주소를 휴대폰 브라우저에 넣습니다. 또는 `docker compose logs tunnel | grep trycloudflare`. 주소는 다시 켤 때마다 바뀌고, 가동 보장이 없는 무료 임시 주소입니다. 고정 주소는 아래 "선택: 고정 도메인".
- 정기 수집은 30분마다, 키워드 다시 만들기는 매일 06:00, 백업은 매일 04:00(이 폴더의 `backups/`, 최근 7개)입니다. 노트북이 꺼져 있으면 멈추고, 켜지면 놓친 것을 바로 한 번 합니다.
- 설정(수집 간격, 월 예산 한도 등)은 웹의 '설정' 메뉴에서 바꿉니다. 월 예산 기본값은 $5이고, 넘으면 AI 호출만 멈춥니다.

## 선택: 고정 도메인

Cloudflare 계정에 내 도메인이 있으면 바뀌지 않는 주소를 쓸 수 있습니다. 메뉴 이름은 Cloudflare 공식 문서(2026-11-07 확인) 기준이며 바뀔 수 있습니다.

1. Cloudflare 대시보드에서 **Networking** > **Tunnels** > **Create a tunnel**. 이름을 넣고 **Create Tunnel**.
2. 운영 체제를 고르는 화면에서 **Docker**를 고르면 나오는 명령의 `--token` 뒤 긴 문자열이 토큰입니다(다른 운영 체제 명령에 붙은 긴 문자열도 같은 값). 복사만 하고 설치 명령은 실행하지 않습니다(이 앱의 `tunnel` 컨테이너가 연결합니다).
3. 터널의 **Routes** 탭에서 **Add route** > **Published application**. 서브도메인과 **Domain**을 고르고(예: `ripple` + `example.com`), **Service URL**에 `http://web:3000`을 넣고 **Add route**.
4. 설정 파일(위 3단계에서 만든 것)의 `TUNNEL_TOKEN=`·`PUBLIC_URL=` 줄을 채웁니다.

   ```
   TUNNEL_TOKEN=복사한 토큰
   PUBLIC_URL=https://ripple.example.com
   ```
5. `docker compose up -d`

확인: http://127.0.0.1:3000/status 의 외부 접속에 고정 주소와 "연결됨(연결 N개)"이 보이고, 휴대폰에서 그 주소로 열립니다.

- `TUNNEL_TOKEN`은 비밀 값입니다. 설정 파일에만 넣고 남에게 보이지 마세요. 새면 대시보드에서 토큰을 새로 만들어 바꿉니다.
- 같은 토큰을 두 컴퓨터(또는 두 Docker)에서 쓰면 접속이 둘로 나뉘어 어떤 때는 열리지 않습니다.
- 토큰을 비우고 `docker compose up -d`하면 임시 주소로 돌아갑니다.

## 손님 보기

주소를 아는 사람은 누구나 비밀번호 없이 타임라인과 종목 화면을 볼 수 있습니다(손님 보기). 보는 것만 되고, 상태·설정 화면과 모든 수정은 막힙니다.

- 끄려면 `.env`에 `GUEST_VIEW=off`를 넣고 `docker compose up -d`. 그러면 주소를 아는 사람도 로그인부터 해야 합니다.
- 손님이 종목 등록·삭제처럼 수정을 하면 비밀번호 창이 뜹니다. 맞는 비밀번호를 넣으면 하려던 일이 바로 실행되고, 그 뒤 `EDIT_UNLOCK_MINUTES`(기본 30분) 동안은 다시 묻지 않습니다. 시간이 지나면 다시 묻습니다.
- 손님의 읽음 표시는 그 브라우저에만 저장됩니다(내 읽음 기록은 바뀌지 않음).
- 손님은 접속자(IP)별로 1분에 60번까지, 타임라인은 최대 90개까지 볼 수 있습니다.
- 이 기능이 들어간 버전으로 업데이트한 뒤에는 한 번 다시 로그인해야 합니다.

### 선택: Cloudflare 요청 제한 규칙

공개 주소로 너무 많은 요청이 들어오는 것을 Cloudflare에서 먼저 막고 싶을 때 씁니다. 무료 요금제는 규칙 1개, 센 기간 10초, 막는 시간 10초, 센 기준 IP만 쓸 수 있습니다(Cloudflare 문서의 요금제별 표). 모든 경로에 IP별로 10초에 50번을 넘으면 10초 막는 규칙 하나를 만듭니다.

1. Cloudflare 대시보드에서 도메인을 고르고 **Security rules**를 엽니다.
2. **Create rule** → **Rate limiting rules**를 누릅니다.
3. **Rule name**에 이름을 적습니다(예: `ripple-rate-limit`).
4. 모든 경로에 걸리도록 조건을 정합니다. 모든 요청에 맞추는 Field·Operator·Value 값은 대시보드에서 확인합니다.
5. **With the same characteristics**에서 IP를 고릅니다.
6. **When rate exceeds**에 요청 수 `50`, 기간 `10 seconds`를 넣습니다.
7. **Then take action**의 **Choose action**에서 `Block`을 고릅니다.
8. **Duration**을 `10 seconds`로 하고 **Deploy**를 누릅니다.

메뉴 이름은 Cloudflare 문서(https://developers.cloudflare.com/waf/rate-limiting-rules/create-zone-dashboard/)를 따랐습니다. 화면이 다르면 대시보드 안내를 따르세요.

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

**v0.2.0 이하에서 v0.3.0 이후로는 업데이트할 수 없습니다.** v0.3.0부터 앱을 Rust로 다시 만들면서 옛 DB를 이어받지 못합니다. 그대로 켜면 `migrate`가 "drizzle 이력이 기대와 다릅니다"로 멈추고(DB는 바뀌지 않음) web·worker가 뜨지 않습니다. 옛 버전을 계속 쓰거나, 데이터(종목·기사·설정)를 버리고 새로 설치합니다:

```bash
docker compose down
docker volume rm ripple_ripple-data   # DB를 지운다(되돌릴 수 없음). backups/ 폴더는 남는다
docker compose up -d                   # 새 버전의 docker-compose.yml로
```

## 백업 복원

`backups/` 폴더의 `ripple-날짜-시각.db`로 되돌립니다(종목·기사·설정이 그 시각으로 돌아감).

```bash
docker compose down
docker run --rm -v ripple_ripple-data:/data -v "$PWD/backups":/backups:ro ripple-app:__VERSION__ \
  sh -c 'cp /backups/되돌릴파일.db /data/ripple.db && rm -f /data/ripple.db-wal /data/ripple.db-shm'
docker compose up -d
```

## 문제가 생기면

| 증상 | 확인 |
|---|---|
| web이 계속 재시작 | `docker compose logs web`의 `[설정 오류]`(비밀번호·키 길이 부족) |
| `no matching manifest`·`exec format error` | 내 컴퓨터와 다른 종류(arm64/amd64) 파일을 불러온 것입니다. 맞는 파일로 다시 `docker load` |
| 휴대폰에서 주소가 안 열림 | 컴퓨터가 켜져 있는지, 주소가 바뀌지 않았는지(상태 화면), 안 되면 `docker compose restart tunnel`(임시 주소면 새 주소가 생김) |
| 상태 화면에 "Cloudflare와 연결되지 않았습니다" | 고정 주소: `TUNNEL_TOKEN` 값이 맞는지, `docker compose logs tunnel`, 같은 토큰을 다른 곳에서 쓰는지 |
| 상태 화면에 "터널에 닿지 못했습니다: …" | `docker compose ps`로 tunnel이 떠 있는지, `docker compose up -d` |
| 상태 화면에 "PUBLIC_URL 형식이 다릅니다(https://호스트 꼴)" | `PUBLIC_URL`을 `https://ripple.example.com`처럼 고칩니다(끝에 `/`·경로 없이) |
| 고정 주소로 열면 Cloudflare 오류(502 등) | 대시보드 Routes의 Service URL이 `http://web:3000`인지 |
| 기사가 안 보임 | 웹 '상태' 화면의 최근 수집 결과, 평가 키, 월 예산, '설정'의 관련도 기준 |

소스와 자세한 설명: https://github.com/anjfgkfkrh/Ripple
