# Ripple

보유 주식과 시장 전체에 관련된 뉴스를 자동으로 모으고 Jev(TypeSafe AI)로 관련도·영향도와 호재·악재를 골라 웹으로 보여 주는 개인용 앱. 집 노트북의 Docker에서 돌고, 휴대폰에서는 Cloudflare 임시 주소(또는 내 도메인의 고정 주소)로 접속한다.

- 기능 명세: [docs/SPEC.md](docs/SPEC.md)
- 지금 상태: [docs/STATE.md](docs/STATE.md)
- 개발 방법과 단계 현황: [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)
- 구현 세부 결정·실측·진행 기록: [docs/DECISIONS.md](docs/DECISIONS.md)

## 하는 일

1. 종목을 등록하면(예: 삼전, NVDA) AI가 연관 키워드 후보 트리를 만들고 Jev가 관련도·영향도로 골라 남긴다(매일 06:00에 그 종목의 최근 뉴스를 참고해 다시 만듦). 트리는 제품·고객·경쟁사처럼 잘 바뀌지 않는 요인만 담고, 지금 사건은 시장 토픽이 맡는다.
2. 종목과 별개로 '한국 증시'(기본으로 켜짐, 미국 증시는 켜고 끄기)와 직접 넣은 주제(예: 부동산 시장)도 같은 방식으로 기사를 모은다.
3. 3시간마다(설정) 모은 기사 제목으로 AI가 지금 시장을 움직이는 토픽을 다시 잡고, 영향을 받을 내 종목에 그 토픽 가지를 붙인다.
4. 30분마다 언론사 RSS 29곳과 Finnhub(미국 종목)에서 종목 이름·키워드가 들어간 기사를 모은다.
5. Jev가 기사마다 관련도·영향도(0~10)와 호재·악재·중립을 매긴다. 같은 소식을 여러 언론사가 낸 기사는 한 번만 묻는다.
6. 타임라인(홈) 맨 위에 종목 주가 띠와 '지금 시장 이슈' 토픽 카드를, 아래에 관련도·영향도가 둘 다 6점 이상인 기사를 최신순으로 보여 준다. 항목(종목·시장·주제)·호재/악재로 거를 수 있다.
7. 장중 1분마다 등록 종목의 주가(한국은 한국투자증권 KIS, 미국은 Finnhub)를 받는다.
8. 매일 04:00에 DB를 이 컴퓨터의 `backups` 폴더에 백업한다.

## 1. 처음 설치

준비물: Docker Desktop(compose v2). 개발할 때만 Rust 1.99.1(rustup)과 Node.js 24·pnpm 12가 더 필요하다(실행 파일 빌드와 E2E).

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
| `FINNHUB_API_KEY` | 선택 | 미국 종목 뉴스와 주가(무료 가입 https://finnhub.io). 없으면 미국 뉴스·주가는 건너뛴다 |
| `KIS_APP_KEY`, `KIS_APP_SECRET` | 선택 | 한국 종목 주가(한국투자증권 계좌로 KIS Developers https://apiportal.koreainvestment.com 에서 앱 키 발급). 없으면 한국 종목 주가는 보이지 않는다 |
| `KIS_ENV` | 선택 | KIS 앱 키 종류: `demo`(모의투자, 기본) 또는 `real`(실전). 키가 새어도 실제 계좌로 주문되지 않게 모의투자 키를 권한다 |
| `TUNNEL_TOKEN`, `PUBLIC_URL` | 선택 | 고정 주소용 Cloudflare 터널 토큰(비밀 값)과 그 주소. 비우면 임시 주소. 아래 "선택: 고정 도메인" |
| `GUEST_VIEW` | 선택 | 로그인 안 한 사람(손님)에게 타임라인·종목 보기를 열어 둘지. 비우거나 `on`이면 열림, `off`면 끔. 아래 "손님 보기" |
| `EDIT_UNLOCK_MINUTES` | 선택 | 비밀번호를 넣은 뒤 수정할 수 있는 시간(분). 1~120, 비우면 30 |

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

고정 주소(`https://ripple.example.com` 같은 내 도메인)가 있으면 아래 "선택: 고정 도메인"을, 없으면 임시 주소를 쓴다.

#### 토큰이 없을 때: 임시 주소

1. 이 컴퓨터에서 http://127.0.0.1:3000/status 를 열면 "외부 접속(임시 주소)"에 `https://…trycloudflare.com` 주소가 보인다. 또는 `docker compose logs tunnel | grep trycloudflare`.
2. 휴대폰 브라우저에 그 주소를 넣고 `APP_PASSWORD`로 로그인한다(30일 유지).
3. 주소는 Docker를 다시 켤 때마다(노트북을 다시 켰을 때 포함) 바뀐다. 바뀌면 1번부터 다시.

#### 선택: 고정 도메인

Cloudflare에 내 도메인이 등록돼 있어야 한다. 메뉴 이름은 Cloudflare 공식 문서(2026-11-07 확인) 기준이며 바뀔 수 있다.

1. Cloudflare 대시보드에서 **Networking** > **Tunnels** > **Create a tunnel**. 이름을 넣고 **Create Tunnel**.
2. 운영 체제를 고르는 화면에서 **Docker**를 고르면 나오는 명령의 `--token` 뒤 긴 문자열이 토큰이다(다른 운영 체제 명령에 붙은 긴 문자열도 같은 값). 복사만 하고 설치 명령은 실행하지 않는다(Ripple의 `tunnel` 컨테이너가 대신 연결한다).
3. 만든 터널의 **Routes** 탭에서 **Add route** > **Published application**. 서브도메인과 **Domain**을 고르고(예: `ripple` + `example.com`), **Service URL**에 `http://web:3000`을 넣고 **Add route**.
4. 환경 변수 파일(`.env`)에 토큰과 주소를 넣는다.

   ```
   TUNNEL_TOKEN=복사한 토큰
   PUBLIC_URL=https://ripple.example.com
   ```
5. `docker compose up -d`. 토큰이 있으면 `tunnel` 컨테이너가 다시 만들어진다.

확인: http://127.0.0.1:3000/status 의 외부 접속에 고정 주소와 "연결됨(연결 N개)"이 보이고, 휴대폰에서 그 주소로 열려 로그인된다. 노트북을 껐다 켜도 주소는 그대로다.

주의
- `TUNNEL_TOKEN`은 비밀 값이다. env 파일에만 넣고 출력하거나 커밋하지 않는다. 새면 대시보드에서 토큰을 새로 만들고 env 파일을 고친다.
- 같은 토큰을 두 컴퓨터(또는 두 Docker)에서 쓰지 않는다. Cloudflare가 접속을 둘로 나눠 보내 어떤 때는 열리지 않는다. 시험용은 다른 이름의 터널을 따로 만든다.
- 토큰을 비우고 `docker compose up -d`하면 임시 주소 방식으로 돌아간다.

알아 둘 것
- Cloudflare Quick Tunnel은 계정 없이 무료이지만 공식 문서상 테스트·개발용이라 가동 보장이 없다. 주소가 안 열리면 `docker compose restart tunnel`(새 주소가 생김).
- 노트북이 꺼지거나 절전이면 휴대폰에서도 열리지 않고 수집도 멈춘다. 다시 켜지면 놓친 수집·키워드·백업을 바로 한 번 한다.
- 같은 접속자가 비밀번호를 5번 틀리면 15분 동안 로그인을 받지 않는다. 모든 기기의 로그인을 풀려면 `APP_PASSWORD`나 `SESSION_SECRET`을 바꾸고 `docker compose up -d`.

### 손님 보기

주소를 아는 사람은 누구나 비밀번호 없이 타임라인과 종목 화면을 볼 수 있습니다(손님 보기). 보는 것만 되고, 상태·설정 화면과 모든 수정은 막힙니다.

- 끄려면 `.env`에 `GUEST_VIEW=off`를 넣고 `docker compose up -d`. 그러면 주소를 아는 사람도 로그인부터 해야 합니다.
- 손님이 종목 등록·삭제처럼 수정을 하면 비밀번호 창이 뜹니다. 맞는 비밀번호를 넣으면 하려던 일이 바로 실행되고, 그 뒤 `EDIT_UNLOCK_MINUTES`(기본 30분) 동안은 다시 묻지 않습니다. 시간이 지나면 다시 묻습니다.
- 손님의 읽음 표시는 그 브라우저에만 저장됩니다(내 읽음 기록은 바뀌지 않음).
- 손님은 접속자(IP)별로 1분에 60번까지, 타임라인은 최대 90개까지 볼 수 있습니다.
- 이 기능이 들어간 버전으로 업데이트한 뒤에는 한 번 다시 로그인해야 합니다.

#### 선택: Cloudflare 요청 제한 규칙

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

### 화면

| 메뉴 | 내용 |
|---|---|
| 타임라인(홈) | 맨 위 '지금 시장 이슈' 토픽 카드(누르면 그 토픽 기사만, 12시간 조용하면 '이전 이슈'로 접힘). 관련도·영향도 기준(기본 6·6점) 이상 기사, 최신순. 항목(종목·한국 증시·주제)·영향(호재·악재·중립) 필터, "더 보기"(30개씩). 기사 하나에 카드 하나, 같은 소식(비슷한 제목)은 "외 N곳"으로 묶음. 읽은 기사 표시: 카드의 원문 링크("외 N곳" 포함)를 누르면 그 카드(묶인 기사 모두)가 읽음이 되어 제목이 흐려지고 "읽음"이 붙고, 카드 아래 "안 읽음으로"·"읽음으로 표시" 버튼으로 되돌리거나 직접 표시. 서버에 저장되어 휴대폰·PC가 같음(손님은 그 브라우저에만 저장) |
| 종목 | 이름·티커로 찾아 등록·삭제(삭제는 숨김, 다시 등록하면 되살아남), 키워드 트리 보기(키워드마다 관련도·영향도, 붙은 토픽)·다시 만들기. '시장·주제' 구역: 한국·미국 증시 켜기/끄기, 주제 넣기·지우기 |
| 상태 | 시스템 상태, 외부 접속 임시 주소, 마지막 백업, 수집 현황(최근 회차·출처별 결과·다음 수집 시각), "지금 수집" 버튼, 시장 토픽(마지막·다음 갱신, 실패 이유), "지금 갱신" 버튼 |
| 설정 | 수집 간격, 키워드 깊이(1~2)·정기 점검 주기·토픽을 종목에 붙일 기준·모델, 관련도·영향도 표시 기준, 회차당 평가 수, 토픽 갱신 간격·특정 시각, 월 예산, 이번 달 AI 사용량(기능별·항목별 비용과 비율) |

### 비용

- 실제로 돈이 드는 것은 OpenRouter(키워드·종목 찾기·토픽 갱신)와 TypeSafe(기사·키워드 평가)뿐이다. 수집은 무료다.
- 실측(2026-10-21): 종목 3개 기준 한 달 약 $0.4(평가 약 $0.35, 키워드 약 $0.05). 종목이 많거나 뉴스가 많은 종목일수록 늘어난다.
- 시사 반영(2026-10-22) 뒤 추정: 시장 이슈 정리 하루 약 $0.007(종목 수와 상관없음, 한 달 약 $0.2), 키워드 트리 한 번 약 $0.002(깊이 3 기준, 전에는 약 $0.001), 시사 키워드로 늘어나는 기사 평가 종목당 하루 약 $0.001. 종목 3개면 한 달 약 $0.8. (2026-10-23부터 시장 이슈 정리와 트리의 시사 키워드는 없어지고 시장 토픽이 대신함)
- 시장 토픽(2026-10-22) 뒤 추정(실측 단가로 계산): 토픽 갱신은 낮에 1번 약 $0.004~0.005(3시간마다면 한 달 많아야 약 $1.2, 밤에는 새 제목이 적어 더 싸거나 건너뜀), 키워드 트리 한 번 약 $0.003(후보 트리 + 후보마다 Jev 평가), 한국 증시·토픽 기사 평가 하루 약 $0.02. 종목 3개면 한 달 약 $3. 설정 화면에서 기능별·항목별로 얼마를 썼는지 볼 수 있다.
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
| 휴대폰에서 주소가 안 열림 | 노트북이 켜져 있는지, 주소가 바뀌지 않았는지(상태 화면), `docker compose logs tunnel`. 안 되면 `docker compose restart tunnel`(임시 주소면 새 주소가 생김) |
| 상태 화면에 "Cloudflare와 연결되지 않았습니다" | 고정 주소: `TUNNEL_TOKEN` 값이 맞는지, `docker compose logs tunnel`, 같은 토큰을 다른 곳에서 쓰는지 |
| 상태 화면에 "터널에 닿지 못했습니다: …" | `docker compose ps`로 tunnel이 떠 있는지, `docker compose up -d` |
| 상태 화면에 "PUBLIC_URL 형식이 다릅니다(https://호스트 꼴)" | `PUBLIC_URL`을 `https://ripple.example.com`처럼 고친다(끝에 `/`·경로 없이) |
| 고정 주소로 열면 Cloudflare 오류(502 등) | 대시보드 Routes의 Service URL이 `http://web:3000`인지 |
| 기사가 안 보임 | 상태 화면의 최근 회차(실패한 소스, 평가 결과와 멈춘 이유). 평가 키가 없거나 예산을 다 썼거나, 관련도·영향도 기준(설정)이 높을 수 있다 |
| 토픽 카드가 없음 | 상태 화면의 '시장 토픽'(마지막 갱신, 실패 이유). 새 제목이 20개보다 적으면 토픽을 바꾸지 않는다. 토픽에 기준 이상 기사가 하나도 없으면 카드가 보이지 않는다 |
| 종목 찾기가 안 됨 | 화면의 오류 문구(예산·OpenRouter 키·크레딧). 공식 목록에 없는 종목(한국 우선주·ETF, 미국 장외)은 등록할 수 없다 |
| 백업이 안 생김 | `docker compose logs worker \| grep 백업`. 리눅스에서는 `backups/` 폴더를 uid 1000이 쓸 수 있어야 한다 |
| 로그인이 막힘 | 5번 틀리면 15분 기다리거나 `docker compose restart web` |

## 6. 기능 자세히

### 종목과 연관 키워드
- 입력(삼전, 엔비디아, NVDA 등)으로 AI가 후보를 내고, 공식 종목 목록(KIND, NASDAQ Trader, 7일마다 갱신)에 있는 것만 최대 3개 보여 준다.
- 등록하면 worker가 따로 키워드 트리를 만든다(기본 깊이 2). AI가 단계마다 후보를 5~8개 내고, Jev가 후보마다 관련도·영향도를 매겨 둘 다 기준(기본 6·5, 설정) 이상인 것만 남긴다(부모가 잘리면 자식도, 많아야 40개). Jev를 부를 수 없으면 자르지 않고 남긴다. 한국 종목은 한국어, 미국 종목은 한국어·영어. 키워드마다 윗단계와 이어지는 이유와 경로(예: 삼성전자 → HBM 메모리 → 엔비디아)를 둔다.
- 매일 06:00(설정에서 주 1회로 바꿀 수 있음)에 다시 만든다. LLM은 오늘 뉴스를 모르므로 그 종목의 최근 7일 주요 기사 제목(최대 20개)을 함께 준다. 실패하면 이전 키워드를 둔다.
- 시사(지금 사건): 트리에는 넣지 않는다(2026-10-23). 시장 토픽이 영향을 받을 종목에 토픽 가지를 붙인다(아래 '시장·주제와 시장 토픽').
- 깊이를 바꾸면 다음 06:00에 반영된다. 바로 반영하려면 설정 화면의 "지금 다시 만들기".

### 시장·주제와 시장 토픽
- '한국 증시'는 처음부터 켜져 있고 '미국 증시'는 종목 화면에서 켤 수 있다. 금리·환율·유가·물가·수출·부동산·정책·지정학처럼 시장 전체를 움직이는 요인으로 트리를 만들고, 지수 이름(코스피·증시 등)으로는 기사를 걸지 않는다.
- 주제(2~20자, 개수 제한 없음)를 넣으면 켜진 증시 기준으로 트리를 만든다. 두 증시가 다 켜져 있으면 증시마다 따로 평가한다.
- 시장 토픽: 설정한 간격(기본 3시간, 0이면 끔)이나 특정 시각에, 지난 갱신 이후 모은 제목(종목에 안 걸린 기사 포함, 제목·주소·요약 3일 보관)과 이전 토픽 목록(토픽마다 최근 24시간 기사 수와, 끝난 지 48시간 안의 토픽 포함)으로 AI가 토픽을 최대 15개 다시 잡는다(없으면 적게). 이어지는 토픽은 그대로, 끝난 토픽이 같은 주제로 다시 나오면 되살린다. AI가 목록에서 뺀 토픽도 최근 24시간에 기사가 5건 이상 걸리고 그중 화면 기준을 통과한 기사가 있으면 끝내지 않고(15개가 넘으면 화면 기준 통과 기사가 적은 것부터 끝냄), 그렇지 않은 식은 토픽은 끝낸다(끝난 토픽에는 새 기사를 걸지 않고, 카드는 3일 동안 남음). 새 제목이 20개보다 적으면 바꾸지 않고, 실패하면 30분 뒤 다시 한다.
- 토픽마다 AI가 검색어와 영향을 받을 내 종목·이유를 내고, Jev가 토픽–종목 쌍을 평가해 기준을 넘으면 그 종목 트리에 토픽 가지를 붙인다(떨어지거나 토픽이 끝나면 뗌). 새 토픽과 새로 붙인 가지에는 지난 24시간 기사를 바로 건다(평가는 다음 수집 때).

### 기사 수집
- 소스: 언론사 RSS 29곳(피드 56개, `rust/ripple/src/server/collect/feeds.rs`)과 Finnhub. 약관·robots.txt가 자동 수집이나 AI 이용을 명시적으로 막는 곳은 넣지 않았다(SPEC 11장). 증권사 리포트는 따로 모으지 않는다.
- 제목과 요약 앞 300자에 종목 이름이나 키워드의 낱말이 모두 있으면 그 종목에 건다. Finnhub 종목 뉴스는 그 종목에 바로 건다. 걸린 기사만 제목·요약(300자)·원문 링크·언론사·시각을 저장한다(본문은 저장하지 않음).
- 시장 토픽용으로 언론사 RSS와 Finnhub 시장 뉴스는 종목에 안 걸린 기사도 제목·언론사·피드·시각·원문 주소·요약(300자)을 3일 둔다(같은 제목은 하나).
- 시장·주제는 이름으로 걸지 않고 키워드로만 건다.
- 처음엔 최근 24시간, 이후엔 지난 회차 시작 1시간 전부터 받는다. 지난번에 실패한 소스는 그 소스가 마지막으로 성공한 회차부터, 새로 등록한 종목은 다음 회차에서 등록 24시간 전 기사까지 건다.
- 발행 시각이 없거나 받은 시각보다 10분 넘게 미래면(국민일보 지면 기사) 처음 받은 시각으로 저장한다.
- 받기 전에 robots.txt를 확인하고, 같은 사이트에는 1.1초 이상 간격을 두고, User-Agent에 개인용 RSS 리더임을 밝힌다. robots.txt를 확인하지 못하면(서버 오류·연결 실패) 그 회차에는 받지 않고 원인을 소스 오류로 남긴다.
- 수집용 HTTP 연결은 OpenSSL(native-tls)을 쓴다. TLS 1.2 CBC 암호만 받는 사이트(노컷뉴스)가 있어서다. AI·주가 등 나머지 연결은 rustls.

### 주가
- 타임라인 맨 위 띠에 등록 종목의 현재가·전일 대비·등락률·기준 시각을 보여 준다(상승 빨강 ▲, 하락 파랑 ▼). 누르면 그 종목 기사만 보인다.
- 한국 종목은 한국투자증권 KIS Open API(통합 시세: KRX+넥스트레이드), 미국 종목은 Finnhub `/quote`. 시세 조회만 하고 주문은 하지 않는다.
- 장중(한국 평일 08:00~20:00, 미국 뉴욕 평일 09:30~16:00)에 1분마다 받아 이력으로 쌓는다. 장 밖에는 마지막 값과 "장 마감"을 보여 준다. 휴장일 달력은 없어 휴장일에도 같은 값을 받는다.
- 화면은 자동으로 새로 고치지 않는다. 새로 고치면 가장 최근 값이 보인다. 받기 상태와 오류는 상태 화면에서 본다.

### 정기 작업
| 작업 | 언제 |
|---|---|
| 기사 수집·평가 | 마지막 성공 회차 + 30분(설정). 예정보다 5분 넘게 늦으면 '누락 보정'. 회차 전체가 실패하면 5분 뒤 다시. 종목·시장·주제가 하나도 없으면 쉰다 |
| 키워드 다시 만들기 | 매일 06:00(놓쳤으면 켜질 때 한 번) |
| 시장 토픽 갱신 | 마지막 갱신 + 3시간(설정)이나 정한 시각 중 먼저 오는 때(놓쳤으면 켜질 때 한 번). 실패하면 30분 뒤 다시 |
| 주가 | 1분마다 확인, 장중인 시장의 종목만(한 번도 못 받은 종목은 장 밖이어도 한 번) |
| 백업 | 매일 04:00(놓쳤으면 켜질 때 한 번) |
| 공식 종목 목록 | 30분마다 확인, 7일 지났을 때만 받음 |

### 기사 평가(Jev)
- 기사–종목 연결마다 관련도·영향도(상황 6단계 × 2 = 0~10)와 호재·악재·중립을 한 요청으로 묻는다. 시장·주제는 증시 전체에 영향을 줄 소식인지로 묻는다. 모델은 `jev-1.13.0`(버전 고정, 바꿀 때는 품질을 다시 비교).
- 관련도와 영향도가 둘 다 기준(기본 6·6) 이상인 기사만 타임라인에 보인다. 같은 항목에 24시간 안에 걸린 비슷한 제목의 기사는 앞선 평가를 그대로 쓴다(실측 약 21% 덜 부름).
- 회차당 최대 200건(설정). 넘는 것은 다음 회차로, 종목 이름·직접 연결 먼저, 최근 기사부터, 최근 7일 기사만.
- 키가 없거나 맞지 않을 때, 예산을 다 썼을 때, 연달아 3번 실패했을 때는 그 회차 평가를 멈추고 다음 회차에 이어서 한다.

## 7. 다른 사용자에게 배포하기(릴리스 이미지)

저장소를 그대로 받아 `docker compose up -d --build`로 설치하는 방법 말고, 미리 만든 이미지를 GitHub 릴리스 페이지에 파일로 올려 배포할 수 있다. 받는 사람은 빌드 없이 `docker load` → `.env` 채움 → `docker compose up -d`만 하면 된다. GitHub Actions·ghcr.io는 쓰지 않는다.

```bash
scripts/release.sh            # release-dist/ 에 만든다(약 11분, arm64·amd64 두 종류)
ARCHS=arm64 scripts/release.sh   # 한 종류만 다시 만들 때
```

`release-dist/`에 생기는 파일(git에는 올라가지 않음)을 `gh release create`로 릴리스 페이지에 올린다.

| 파일 | 설명 |
|---|---|
| `ripple-버전-images-arm64.tar.gz` | Apple Silicon Mac용(이미지 하나, migrate·worker·web이 같이 씀) |
| `ripple-버전-images-amd64.tar.gz` | Intel·AMD PC(Windows, Linux, Intel Mac)용 |
| `docker-compose.yml` | 설치용(본 compose에서 `build:`를 `image:`로 자동 변환) |
| `env.example`, `INSTALL.md`, `SHA256SUMS` | 환경변수 예시(`.env.example`과 같은 내용, GitHub 릴리스가 점으로 시작하는 이름의 점을 지워서 점 없이 올림), 받는 사람용 설치 안내(`release/INSTALL.md`가 원본), 체크섬 |

스크립트가 이미지 안을 검사한다: 종류(arm64/amd64)가 맞는지, 환경변수·`.env`·DB 파일이 없는지, 내 `.env`의 비밀 값이 이미지 어디에도 없는지, 그 종류에서 마이그레이션이 실제로 도는지. 하나라도 어긋나면 멈춘다.

한계: amd64 이미지는 이 Mac에서 에뮬레이션으로 만들고 마이그레이션까지만 확인했다. 실제 amd64 PC에서 전체를 켜 본 것은 아니다.

## 8. 개발

자세한 현황·규칙·검사 절차는 [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md). Claude Code(터미널)로 개발한다. 단계 작업은 agentflow 플러그인(`/agentflow:plan`·`run`·`close`, 단계 작업 폴더 `work/`, 역할별 모델 `.agentflow/models.json`)으로 한다. 규칙은 `CLAUDE.md`, 권한·hook·skill(`/check`, `/deploy` 등)은 `.claude/`(DEVELOPMENT.md 4장).

```bash
cp .env.example .env                 # 위 "처음 설치"대로 값을 넣는다
(cd rust && ./build.sh)              # CSS와 실행 파일(debug) 만들기. 고친 뒤마다 다시
set -a && . ./.env && set +a         # 실행 파일은 .env를 직접 읽지 않으므로 셸에 값을 올린다
export DATABASE_PATH=./data/ripple.db
rust/target/debug/ripple migrate     # ./data/ripple.db 생성·마이그레이션 적용
WORKER_HOST=127.0.0.1 rust/target/debug/ripple worker   # 터미널 1: worker  http://127.0.0.1:4000
HOSTNAME=127.0.0.1 rust/target/debug/ripple web         # 터미널 2: web     http://127.0.0.1:3000
```

- Docker의 web도 3000번을 쓰므로 함께 띄우지 않는다(또는 Docker 쪽을 `WEB_PORT`로 바꾼다).
- 스키마 변경: `rust/ripple/migrations/`에 다음 번호의 SQL 파일을 두고, `rust/ripple/src/server/db/migrate.rs`의 `MIGRATION_SQL` 목록에 넣은 뒤 `cargo test`로 확인한다.
- 화면 부품: `rust/ripple/templates/`의 Askama 템플릿(htmx로 조각을 바꾼다). 스타일은 Tailwind 클래스.
- 환경변수는 `.env.example`. Docker에서는 compose가 컨테이너용 값을 정하고 비밀 값만 `.env`에서 필요한 컨테이너에 넘긴다(로그인 값은 web, AI·Finnhub·KIS 키는 worker, 내부 토큰은 둘 다).

### 검사·테스트

| 명령 | 내용 |
|---|---|
| `cd rust && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` | Rust 검사와 단위·통합 테스트(실제 마이그레이션으로 만든 임시 DB) |
| `pnpm typecheck` / `pnpm format:check` / `pnpm lint` | E2E·스크립트의 타입, 서식, ESLint(typescript-eslint) |
| `pnpm test` | Vitest(커밋 전 비밀 값 검사 스크립트의 테스트) |
| `cd rust && ./build.sh --release` | E2E 전에 release 실행 파일을 새로 만든다(옛 파일이면 `/`가 404) |
| `pnpm test:e2e` | Playwright(데스크톱·Pixel 7). 가짜 외부 서비스(4200)와 시험용 worker(4100)·web(3100)을 띄워 화면을 검사하고 `test-results/`에 캡처한다. 유료 API나 실제 언론사를 부르지 않는다 |
| `node scripts/check-secrets.mjs` | 커밋 전: 스테이징한 변경에 `.env`의 비밀 값·흔한 키 형식이 없는지 본다(값은 출력하지 않음). 푸시 전에는 `--history`로 모든 커밋 기록 |

처음 쓸 때는 `pnpm exec playwright install chromium`.

### 시험용 Docker

본 Docker(프로젝트 `ripple`)에는 실제 데이터가 있으므로, Docker 화면을 시험할 때는 볼륨·포트·백업 폴더를 나눈 시험용 프로젝트로 띄운다. 터널도 함께 뜨므로 공개 주소가 생긴다는 점에 주의한다(외부에서 확인할 때는 공개된 시험 비밀번호 대신 무작위 비밀번호를 쓴다).

```bash
# 시험용 띄우기: 포트 3300, 백업 .check-backups, AI 키 비움(비용 없음), 시험 비밀번호
# TUNNEL_TOKEN·PUBLIC_URL도 비운다(본 터널 토큰으로 시험 tunnel이 뜨면 Cloudflare가 실제 접속을 둘로 나눠 보낸다)
TUNNEL_TOKEN= PUBLIC_URL= WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups OPENROUTER_API_KEY= TYPESAFE_API_KEY= FINNHUB_API_KEY= KIS_APP_KEY= KIS_APP_SECRET= \
  APP_PASSWORD=e2e-password-not-secret SESSION_SECRET=e2e-session-secret-not-for-production-use \
  docker compose -p ripple-check up -d --build
E2E_BASE_URL=http://127.0.0.1:3300 pnpm test:e2e   # 가짜 서비스·시험 DB가 필요한 테스트는 건너뜀
# 내리기(시험 볼륨까지 지움)
TUNNEL_TOKEN= PUBLIC_URL= WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups docker compose -p ripple-check down -v && rm -rf .check-backups
```

### 폴더 구조

```
rust/ripple/src/main.rs     실행 파일 진입점: migrate · worker · web · healthcheck
rust/ripple/src/server/     web·worker 공용 코드(설정·시간·DB·상태·백업·터널 주소·설정 값·같은 소식 판단)
rust/ripple/src/server/auth/    로그인: 설정 확인, 세션 쿠키, 실패 제한, worker 내부 토큰
rust/ripple/src/server/ai/llm/  생성형 LLM 인터페이스, OpenRouter 구현, 구조화 출력 스키마
rust/ripple/src/server/ai/decider/ 기사 평가(Jev) 인터페이스, 질문 문구, TypeSafe 구현
rust/ripple/src/server/budget/  AI 사용량 기록과 월 한도 검사(AI 호출은 반드시 여기를 거침), 이번 달 사용량 보고
rust/ripple/src/server/listings/ 공식 종목 목록 받기·읽기(KIND, NASDAQ Trader)
rust/ripple/src/server/stocks/  종목 매칭(LLM + 공식 목록 대조)·등록·삭제
rust/ripple/src/server/keywords/ 연관 키워드: 프롬프트·트리 검증, 참고할 최근 기사 제목, 생성·Jev 평가·저장, worker 대기열
rust/ripple/src/server/collect/ 기사 수집: RSS(feeds.rs 목록), 요청 간격, robots.txt, 종목 연결, Finnhub, 피드 제목 보관, 지난 기사 걸기, 회차 실행
rust/ripple/src/server/evaluate/ 기사 평가 흐름(같은 소식 점수 재사용)
rust/ripple/src/server/topics/  시장·주제 항목과 토픽 저장, 토픽 갱신(계획·프롬프트·실행), 화면 읽기
rust/ripple/src/server/prices/  주가(13단계): KIS 클라이언트, 장 시간, 받기 실행, 저장·화면 읽기
rust/ripple/src/server/timeline/ 타임라인 읽기(필터·같은 소식 묶기·더 보기·토픽 카드)
rust/ripple/src/server/schedule/ 정기 작업(다음 시각 계산, 1분 루프)
rust/ripple/src/server/db/      스키마, 연결, 마이그레이션 적용(MIGRATION_SQL 목록)
rust/ripple/src/worker/         worker 진입점, 내부 API, 마이그레이션 진입점
rust/ripple/src/web/            화면(axum): 로그인·상태·타임라인·종목·설정, 정적 파일
rust/ripple/templates/          화면 템플릿(Askama·htmx), 공통 틀·부품(ui/)
rust/ripple/styles/             Tailwind 입력 CSS와 테마 복사본(node_modules 없이 빌드)
rust/ripple/static/             app.js·htmx·favicon(app.css는 build.sh가 만듦)
rust/ripple/migrations/         마이그레이션 SQL(0000~, 순서대로 적용)
rust/ripple/tests/fixtures/     테스트 표본 파일과 TS 시절 drizzle 고정본(업그레이드 시험용)
rust/build.sh                   CSS → cargo build
e2e/                            Playwright 테스트, 가짜 외부 서비스(mock-services.mjs)
scripts/                        커밋 전 비밀 값 검사, 릴리스 묶음
docs/                           기능 명세(SPEC.md), 지금 상태(STATE.md), 작업 방법(DEVELOPMENT.md), 구현 세부 결정·실측·진행 기록(DECISIONS.md), 설계(RUST_MIGRATION.md)
.claude/                        Claude Code 설정(권한, hook)과 skill
.agentflow/                     agentflow 역할별 모델·effort 설정
work/                           진행 중인 agentflow 단계의 계획·브리프·인계(단계를 닫으면 보관소로 옮김)
```

### 참고

- 이 저장소는 14단계(2026-11, Rust 전환)로 Next.js·TypeScript 서버를 걷어냈다. 전환 전 기록(TS 구조)은 로컬 git 태그 `ts-final`에 남아 있다.
- 시간 표시와 스케줄은 한국 시간(Asia/Seoul)이다.
