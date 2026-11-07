# tunnel-mode: 토큰이 있으면 이름 붙은 터널, 상태 화면에 고정 주소
모델: Sonnet / effort: medium | 선행: 없음

## 목표
`TUNNEL_TOKEN`이 있으면 cloudflared가 이름 붙은 터널(`tunnel run`)로, 없으면 지금처럼 Quick Tunnel로 뜬다. `PUBLIC_URL`이 있으면 상태 화면이 그 고정 주소와 cloudflared 연결 여부를 보인다.

## 완료 조건
- `rust/ripple/src/server/tunnel_tests.rs`: `/ready` 200(연결 수 읽음)·503(연결 안 됨)·닿지 않음, `PUBLIC_URL` 형식 검사(맞음·`http://`·경로 붙음·빈 값) 시험 통과
- `rust/ripple/src/web/config_tests.rs`: `PUBLIC_URL` 읽기(앞뒤 공백 제거, 빈 값은 없음) 시험 통과
- `rust/ripple/src/web/status_tests.rs`: 고정 주소 모드 화면(제목 "외부 접속(고정 주소)", 주소 링크, 연결됨/끊김/형식 오류 글자) 시험 통과. 기존 임시 주소 모드 시험은 기대값 그대로 통과
- 시험 compose 확인: 작업 폴더에 가짜 값만 든 파일을 만들어 `docker compose -f docker-compose.yml --env-file <그 파일> config`로 토큰 있음·없음 두 경우의 tunnel `command`·`environment`와 web `environment`를 `handoff.md`에 붙인다
- `/check` 전체 통과(E2E `e2e/home.spec.ts`의 "터널을 쓰지 않습니다" 기대 그대로)

## 수정 가능 범위
- `docker-compose.yml`
- `.env.example`
- `rust/ripple/src/server/tunnel.rs`
- `rust/ripple/src/server/tunnel_tests.rs`
- `rust/ripple/src/web/config.rs`
- `rust/ripple/src/web/config_tests.rs`
- `rust/ripple/src/web/state.rs`
- `rust/ripple/src/web/status.rs`
- `rust/ripple/src/web/status_tests.rs`
- `rust/ripple/src/web.rs`
- `rust/ripple/templates/status/system.html`
- `work/커스텀도메인/tunnel-mode/`

## 볼 위치
- `docker-compose.yml`의 `tunnel`·`web` 서비스
- `rust/ripple/src/server/tunnel.rs`의 `fetch_tunnel_status`, `request_hostname`, `HOSTNAME`
- `rust/ripple/src/web/status.rs`의 `system`(터널 상태를 글자로 바꾸는 `match`)
- `rust/ripple/templates/status/system.html`의 `data-testid="tunnel-status"` 줄
- `rust/ripple/src/web/config.rs`의 `read_web_config`
- `docs/DECISIONS.md` 1장 "15단계"·"15단계 계획 세부" 줄
- cloudflared 2026.10.0 소스의 메트릭 `/ready` 처리(GitHub `cloudflare/cloudflared` 태그 `2026.10.0`, `metrics/` 폴더)

## 미리 정한 판단
- compose `tunnel` 명령은 문자열 형식 `tunnel --metrics 0.0.0.0:20241 --url http://web:3000 ${TUNNEL_TOKEN:+run}`, `environment`에 `TUNNEL_TOKEN: ${TUNNEL_TOKEN:-}`.
  이유: 토큰이 있을 때만 `run`이 붙고, 없으면 지금 명령과 같다(상위가 시험 compose로 확인). 버린 대안: 목록 형식(빈 값이 `""` 인자로 남음), compose profile(기존 설치에 `COMPOSE_PROFILES`가 없어 터널이 안 뜸).
- web `environment`에 `PUBLIC_URL: ${PUBLIC_URL:-}`. `TUNNEL_TOKEN`은 web·worker에 넘기지 않는다.
  이유: 비밀 값은 쓰는 서비스에만 준다.
- 상태 화면 모드는 `PUBLIC_URL`이 있는지로 정한다. 없으면 지금 동작(`/quicktunnel`)을 한 글자도 바꾸지 않는다.
  이유: web은 토큰을 모른다. 버린 대안: web에 토큰 존재 여부 변수를 따로 넘기기(값이 하나 늘어남).
- 고정 주소 모드: `PUBLIC_URL`이 `https://호스트`(경로·포트 없음, 끝 `/` 하나는 허용하고 떼어 냄, 호스트는 기존 `HOSTNAME` 정규식)이면 그 주소를 링크로 보이고, `TUNNEL_METRICS_URL`의 `/ready`를 같은 제한 시간(`TUNNEL_TIMEOUT_MS`)으로 묻는다. 200이면 "연결됨(연결 N개)"(본문 `readyConnections`가 없으면 "연결됨"), 그 밖의 상태 코드면 "Cloudflare와 연결되지 않았습니다", 닿지 않으면 기존 오류 문구 꼴 "터널에 닿지 못했습니다: <원인>". `TUNNEL_METRICS_URL`이 없으면 주소만 보이고 "연결 상태를 알 수 없습니다(터널 메트릭 주소 없음)".
  이유: 사용자 결정 4A. 버린 대안: 주소만 표시.
- `PUBLIC_URL` 형식이 틀리면 web은 그대로 뜨고 상태 화면에 "PUBLIC_URL 형식이 다릅니다(https://호스트 꼴)"를 보인다.
  이유: 표시용 값 때문에 사이트를 멈추지 않는다(DECISIONS "15단계 계획 세부" ④).
- 화면 줄 제목: 임시 주소 모드 "외부 접속(임시 주소)"(그대로), 고정 주소 모드 "외부 접속(고정 주소)". 고정 주소 링크 아래 안내 글은 "휴대폰 브라우저에 이 주소를 넣으세요." 한 문장(임시 주소 모드의 "다시 켜면 주소가 바뀝니다"는 넣지 않음). `data-testid="tunnel-status"`는 그대로.
- `.env.example`에 "15단계: 고정 주소(선택)" 묶음을 9단계 묶음 뒤에 둔다: `TUNNEL_TOKEN=`(빈 값, 설명: Cloudflare 터널 토큰, 비우면 임시 주소), `PUBLIC_URL=`(빈 값, 예: `https://example.com`, 상태 화면 표시용). 9단계 묶음의 "터널(tunnel 서비스)" 설명 줄은 그대로 둔다.

## 체크포인트 (각각 검사 통과 후 커밋)
1. `tunnel.rs`에 `/ready` 읽기와 `PUBLIC_URL` 형식 검사 → `tunnel_tests` 통과
2. `config.rs`·`state.rs`·`web.rs`에 `public_url`, `status.rs`·`system.html` 고정 주소 모드 → `config_tests`·`status_tests` 통과
3. `docker-compose.yml`·`.env.example` → 시험 compose 두 경우 결과를 `handoff.md`에 기록, `/check` 전체 통과

## 제약
- 기존 시험의 기대값은 바꾸지 않는다.
- 저장소 루트에서 `--env-file` 없이 `docker compose config`를 돌리지 않는다(실제 비밀 값이 출력된다). 본 Docker(프로젝트 `ripple`)에 compose 명령을 내리지 않는다. 시험 compose 확인은 `config`만 한다(컨테이너를 띄우지 않음).
- `.env`는 읽지 않는다. 가짜 값 파일 이름에 `.env`를 넣지 않는다(hook이 막음, 예: `fake-vars.txt`).
- `docs/`는 고치지 않는다. 명세에 고칠 것이 보이면 `handoff.md`에 적는다.

## 알려진 함정
- `/ready`의 응답 꼴은 이 브리프의 가정(200/503, 본문 `readyConnections`)이다. 소스에서 다르면 소스대로 하고 `handoff.md`에 근거(파일 경로)를 적는다. 엔드포인트 자체가 없으면 상위로.
- 시험 compose 결과에서 토큰 없음 경우의 tunnel `command`가 지금 본 compose의 목록(`tunnel`, `--metrics`, `0.0.0.0:20241`, `--url`, `http://web:3000`)과 다르면 안 된다.

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고) + cloudflared 2026.10.0에 `/ready`가 없을 때, 고정 주소 모드 때문에 기존 E2E 기대를 바꿔야 할 때
