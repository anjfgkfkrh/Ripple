# 커스텀도메인 계획
목표: Cloudflare 이름 붙은 터널(토큰)로 `anjfgkfkrh.com` 고정 주소에 연결하고, 토큰이 없으면 지금처럼 임시 주소를 쓴다(15단계).

> 상태 값: 대기 / 진행 / 질문 / 되돌리기 / 통과 / 반려 / 보고 / 사람
> 시도: 작업을 worker에게 맡긴 횟수(숫자)

| 작업 | 목표 | 완료 조건 | 모델/effort | 선행 | 상태 | 시도 |
|---|---|---|---|---|---|---|
| tunnel-mode | compose 터널 명령을 토큰 유무로 바꾸고, 상태 화면에 고정 주소와 연결 여부를 보인다 | `tunnel_tests`·`config_tests`·`status_tests` 새 시험 통과, 시험 compose 두 경우 `docker compose config` 결과 기록, `/check` 전체 통과 | Sonnet / medium | 없음 | 통과 | 1 |
| tunnel-docs | README·설치 안내·스킬에 고정 주소 사용법과 시험 Docker 토큰 비우기를 적는다 | README·`release/INSTALL.md`·`.env.example` 안내, `deploy`·`real-check` 스킬 수정, `release.sh` 변환 결과 확인 기록, `/check` 빠르게 통과 | Sonnet / medium | tunnel-mode 통과 | 진행 | 1 |

## 사람이 승인한 큰 결정
- 15단계로 둔다. 완료 기준: 휴대폰에서 고정 도메인으로 접속·로그인, 상태 화면에 고정 주소, 다시 켜도 주소 유지(0A).
- 도메인은 Cloudflare에서 산 `anjfgkfkrh.com`(1A).
- 터널은 원격 관리(토큰) 방식, `TUNNEL_TOKEN` 하나(2A). 이유: 경로가 web 하나뿐이라 로컬 설정 파일의 이점이 없고 비밀 값 범위가 좁다.
- 토큰이 없으면 Quick Tunnel 유지(3A). 상태 화면은 `PUBLIC_URL` + 연결 여부(4A). 배포판에도 선택 안내(5A). Cloudflare Access는 쓰지 않음(6A).
- 실제 확인은 시험용 터널(`check.anjfgkfkrh.com`)로 시험 Docker에서 먼저, 그다음 본 Docker 배포(7A).
- 자세한 줄은 `docs/DECISIONS.md` 1장 "15단계"·"15단계 계획 세부".

## 상위가 정한 작은 결정
- compose 명령 끝 `${TUNNEL_TOKEN:+run}`으로 전환(시험 compose 파일로 두 경우 확인함). `TUNNEL_TOKEN`은 tunnel 서비스에만, `PUBLIC_URL`은 web에만 넘긴다.
- web은 토큰을 받지 않으므로 상태 화면의 모드는 `PUBLIC_URL`이 있는지로 정한다.
- 시험 Docker의 시험용 터널 토큰은 저장소 루트의 `check-tunnel.vars`(git 제외)에 두고 `real-check`가 셸 환경으로 읽는다. 이유: 본 `.env`에 넣으면 본 Docker가 다음 재시작 때 시험 터널에 붙는다. 버린 대안: 본 `.env`에 `CHECK_TUNNEL_TOKEN`을 더하고 compose에서 고르기(본 compose에 시험 전용 값이 생김).
- SPEC 본문(2.1·5·6.1·8·9·11장, FR-1의 "임시 URL" 문구)은 review가 작업 통과 때 고친다.

## 작업이 다 통과한 뒤 사람이 할 일 (run이 맡기지 않음)
1. Cloudflare 대시보드에서 터널 두 개를 만든다: 본(`anjfgkfkrh.com` 또는 정한 하위 이름 → `http://web:3000`), 시험(`check.anjfgkfkrh.com` → `http://web:3000`). 순서는 `tunnel-docs`가 README에 쓴 절차대로.
2. 시험 토큰과 주소를 `check-tunnel.vars`에 넣고 `/real-check`로 확인한다: 휴대폰 접속·로그인, 로그인 응답 쿠키 `Secure`, tunnel 재시작 뒤 같은 주소, 상태 화면 "연결됨". 비용 0원(AI 키 비움).
3. 본 토큰·주소를 `.env`에 넣고 `/deploy`. 배포 뒤 휴대폰으로 확인.
