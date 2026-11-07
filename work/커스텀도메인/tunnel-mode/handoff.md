# tunnel-mode 인계
커밋: 13d3799 | 상태: 완료

## 검사 결과 (실행한 명령과 출력 숫자)
| 명령 | 출력 숫자 |
|---|---|
| `cargo test tunnel` (체크포인트 1) | 8 통과, 0 실패 |
| `cargo test web::` (체크포인트 2) | 83 통과, 0 실패 |
| `/check` R1 `cargo fmt --check` | 경고 0 |
| `/check` R2 `cargo clippy --all-targets -- -D warnings` | 경고·오류 0 |
| `/check` R3 `cargo test` | 601 통과, 0 실패, 0 건너뜀 |
| `/check` R4 `./build.sh --release` | 오류 0 |
| `/check` `pnpm format:check` / `lint` / `typecheck` | 경고·오류 0 |
| `/check` `pnpm test` | 7 통과, 0 실패 |
| `/check` `pnpm test:e2e` | 39 통과, 0 실패, 5 건너뜀(`[mobile]`) |

### 시험 compose (`docker compose -f docker-compose.yml --env-file <가짜 값 파일> config`, config만)
토큰 있음(`fake-vars-on.txt`: TUNNEL_TOKEN=fake-token-abc, PUBLIC_URL=https://example.com)
```
tunnel:
  command: [tunnel, --metrics, 0.0.0.0:20241, --url, http://web:3000, run]
  environment: {TUNNEL_TOKEN: fake-token-abc}
web.environment: PUBLIC_URL: https://example.com, TUNNEL_METRICS_URL: http://tunnel:20241 (TUNNEL_TOKEN 없음)
```
토큰 없음(`fake-vars-off.txt`: APP_PASSWORD만)
```
tunnel:
  command: [tunnel, --metrics, 0.0.0.0:20241, --url, http://web:3000]   # 기존 목록과 같음
  environment: {TUNNEL_TOKEN: ""}
web.environment: PUBLIC_URL: "", TUNNEL_METRICS_URL: http://tunnel:20241 (TUNNEL_TOKEN 없음)
```

## 브리프에 없던 결정
- 범위 밖: 없음. `rust/ripple/src/web.rs`의 시험 도우미에 `start_web_with_tunnel(worker, metrics, public_url)`을 더함(범위 안 파일, `start_web`은 이것을 부르는 얇은 겹).
- `status.rs`에서 `PUBLIC_URL` 형식이 틀리면 연결 상태는 묻지 않고 형식 오류 문구만 보임(링크 없음).
- 고정 주소 모드의 연결 글자는 링크 아래 둘째 줄(span)로 보임.
- `SystemView`에 `tunnel_fixed: bool` 추가(제목·안내 글 전환용).
- 작업 폴더에 가짜 값 파일 `fake-vars-on.txt`·`fake-vars-off.txt`를 남김.

## 발견한 것 (함정, 실패 교훈, 기존 코드의 버그)
- `/ready` 가정이 맞음: 200이면 연결 1개 이상, 아니면 503, 본문 `{status, readyConnections, connectorId}` (cloudflared 2026.10.0 `metrics/readiness.go`, GitHub 태그 확인).
- 토큰 없음일 때 compose가 `TUNNEL_TOKEN: ""`(빈 값 환경 변수)를 cloudflared에 넘김. 빈 값이 Quick Tunnel 동작에 영향이 없는지는 실제 컨테이너로 확인하지 못함(config만 허용). 상위가 `/real-check`로 확인 필요.
- `work/metrics.csv`는 이 작업 밖의 추적 안 된 파일(건드리지 않음).

## 상위로 올린 것
없음.

## 다음 작업에 영향
- 명세에 고칠 것(상위): SPEC에 상태 화면 고정 주소 모드와 `PUBLIC_URL`·`TUNNEL_TOKEN` 설명 필요.
