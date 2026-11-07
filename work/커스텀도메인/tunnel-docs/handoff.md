# tunnel-docs 인계
커밋: d8d269d | 상태: 완료

## 검사 결과 (실행한 명령과 출력 숫자)
| 명령 | 출력 숫자 |
|---|---|
| `pnpm format:check` (체크포인트 1·2 모두) | 경고 0 (All matched files use Prettier code style) |
| `pnpm lint` | 경고·오류 0 |
| `pnpm typecheck` | 오류 0 |
| `pnpm test` | 7 통과, 0 실패 |
| `cargo fmt --check` | 차이 0 |
| clippy·cargo test·빌드·E2E | 돌리지 않음(Rust·화면 코드 변경 없음, `/check 빠르게` 범위 밖) |

### 셸 빈 값 덮어쓰기 (`docker compose -f docker-compose.yml --env-file work/커스텀도메인/tunnel-docs/fake-vars-on.txt config`, 파일: TUNNEL_TOKEN=fake-token-abc, PUBLIC_URL=https://example.com)
셸에 `TUNNEL_TOKEN= PUBLIC_URL=`를 붙임: 가정이 맞음. 파일의 토큰을 덮어 `run`이 없고 `PUBLIC_URL: ""`.
```
tunnel:
  command: [tunnel, --metrics, 0.0.0.0:20241, --url, http://web:3000]
web.environment: PUBLIC_URL: ""
```
셸 변수 없음(대조): `command`에 `run`이 붙음.
```
tunnel:
  command: [tunnel, --metrics, 0.0.0.0:20241, --url, http://web:3000, run]
```

### release.sh 변환 (python 부분 복사 `conv.py`, 출력만 `dist-compose.yml`로, `VERSION=0.0.0 python3 -I conv.py`)
종료 코드 0, 오류 없음. tunnel 줄 유지:
```
command: tunnel --metrics 0.0.0.0:20241 --url http://web:3000 ${TUNNEL_TOKEN:+run}
environment:
  TUNNEL_TOKEN: ${TUNNEL_TOKEN:-}
```
`release-dist/`는 건드리지 않음.

## 브리프에 없던 결정
- 범위 밖: 없음.
- Cloudflare 공식 문서 https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/get-started/create-remote-tunnel/ 를 2026-11-07에 읽음. 메뉴는 **Networking** > **Tunnels** > **Create a tunnel** → **Create Tunnel**, 라우트는 터널의 **Routes** 탭 > **Add route** > **Published application**(서브도메인·**Domain** 선택) > **Service URL**. 문서에 "서비스 종류" 칸은 없고 프로토콜을 Service URL에 쓴다(브리프의 "종류 HTTP, 주소 web:3000"을 `http://web:3000` 하나로 씀). 문서(WebFetch 요약)는 토큰 위치와 비밀 여부를 말하지 않아, "설치 명령의 `--token` 뒤 문자열"은 문서가 아니라 알려진 대시보드 동작에 근거함. 상위가 실제 대시보드로 확인 필요.
- 명령에 env 파일 이름을 쓰면 hook이 막아 README는 "환경 변수 파일"로, INSTALL은 "설정 파일"로 표현한 곳이 있음(같은 파일).
- README 소개 첫 문장에 "(또는 내 도메인의 고정 주소)"를 더함.
- 문제 해결 표에 연결 상태 글자 3줄과 502 줄을 더함(README·INSTALL 동일).
- `real-check`: 종류 표에 "고정 주소" 행, 4-1장("고정 주소 확인")을 더함. 임시 주소 모드에서 "토큰 비움에서 임시 주소가 뜬다"와 고정 주소에서 "tunnel 로그에 ingress 경고·오류 없음" 확인 항목을 넣음. 고정 주소 확인용 비밀번호·서명 키는 `<무작위 …>` 자리표시로 씀.
- `.gitignore`에 `check-tunnel.vars`와 주석 한 줄을 더함.
- 작업 폴더에 `fake-vars-on.txt`(tunnel-mode 것을 복사)·`conv.py`·`dist-compose.yml`을 남김.

## 발견한 것 (함정, 실패 교훈, 기존 코드의 버그)
- 셸의 빈 값 `TUNNEL_TOKEN=`은 `--env-file` 값을 덮는다(위 config 결과). 시험 Docker의 tunnel은 항상 Quick Tunnel로 뜬다.
- 실제 컨테이너로는 확인하지 못함: 빈 `TUNNEL_TOKEN`에서 임시 주소가 뜨는지, 고정 주소의 ingress 경고(`/real-check`에서 확인할 항목으로 스킬에 적음).

## 상위로 올린 것
없음.

## 다음 작업에 영향
- 명세(docs)에 고칠 것(상위): 고정 도메인 절차와 `check-tunnel.vars` 관례를 DEVELOPMENT 시험 Docker 절에 반영. 토큰 위치 문구는 실제 대시보드로 확인.
- `work/metrics.csv`류는 이 작업 밖 파일(건드리지 않음).
