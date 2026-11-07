# tunnel-docs: 고정 주소 안내와 스킬 수정
모델: Sonnet / effort: medium | 선행: tunnel-mode 통과

## 목표
README와 배포판 설치 안내에 "선택: 고정 도메인" 절차를 넣고, `deploy`·`real-check` 스킬이 이름 붙은 터널을 다루게 고친다.

## 완료 조건
- `README.md`·`release/INSTALL.md`에 고정 주소 절차(대시보드에서 터널 만들기 → 공개 호스트 이름을 `http://web:3000`으로 → 토큰과 주소를 env 파일에 → `docker compose up -d`)와 확인 방법, 문제 해결 줄이 있다. 임시 주소 안내는 "토큰이 없을 때"로 남는다
- `real-check` 스킬: 모든 compose 명령 앞에 `TUNNEL_TOKEN= PUBLIC_URL=`가 붙고, "고정 주소 확인" 절이 있다. 가짜 값 파일로 "파일에는 토큰, 셸에는 빈 값"일 때 `config` 결과에 `run`이 없음을 `handoff.md`에 붙인다
- `deploy` 스킬: 이번 변경 뒤 첫 배포에서 tunnel이 다시 만들어진다는 것과 고정 주소 모드의 주소 확인 방법이 있다
- `release.sh`의 compose 변환을 작업 폴더 출력으로 돌린 결과(오류 없음, tunnel `command` 줄 유지)를 `handoff.md`에 붙인다
- `/check 빠르게` 통과

## 수정 가능 범위
- `README.md`
- `release/INSTALL.md`
- `.claude/skills/deploy/SKILL.md`
- `.claude/skills/real-check/SKILL.md`
- `.gitignore`
- `work/커스텀도메인/tunnel-docs/`

## 볼 위치
- `README.md`의 외부 접속(휴대폰) 부분과 명령 표
- `release/INSTALL.md`의 휴대폰 접속 부분, 환경 변수 표, 문제 해결 표
- `.claude/skills/deploy/SKILL.md`의 주소 확인 명령과 "tunnel은 다시 만들어지지 않는다" 문장들
- `.claude/skills/real-check/SKILL.md`의 compose 명령 앞머리 안내와 공개 주소(tunnel) 문단
- `scripts/release.sh`의 "설치용 파일" python 부분
- `docker-compose.yml`의 `tunnel` 서비스, `.env.example`의 "15단계" 묶음(tunnel-mode 결과)
- `work/커스텀도메인/tunnel-mode/handoff.md`
- Cloudflare 공식 문서의 원격 관리 터널 만들기 페이지(developers.cloudflare.com, Cloudflare One → Tunnel → 대시보드로 만들기)

## 미리 정한 판단
- 대시보드 절차는 Cloudflare 공식 문서를 읽고 그 메뉴 이름대로 쓴다. 문서 주소와 읽은 날짜를 `handoff.md`에 적는다. 연결 대상은 서비스 종류 HTTP, 주소 `web:3000`.
  이유: 메뉴 이름이 자주 바뀌어 기억으로 쓰면 틀린다.
- README·INSTALL에는 사용자 도메인을 쓰지 않고 `<도메인>`·`example.com`으로 쓴다.
  이유: 저장소와 배포판은 다른 사람도 읽는다.
- 토큰은 비밀 값이라고 적는다: env 파일에만 넣고, 새면 대시보드에서 토큰을 새로 만들라고 안내한다. 같은 토큰을 두 컴퓨터(또는 두 Docker)에서 쓰면 접속이 나뉘어 간다는 주의를 넣는다.
- `real-check` 기본: compose 명령 앞머리를 `TUNNEL_TOKEN= PUBLIC_URL= WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups docker compose -p ripple-check …`로 바꾼다(KIS 키를 비우는 것과 같은 이유를 한 줄로).
  이유: 본 토큰으로 시험 Docker의 tunnel이 뜨면 Cloudflare가 실제 접속을 시험 Docker로도 보낸다.
- `real-check` "고정 주소 확인" 절: 저장소 루트 `check-tunnel.vars`(사용자가 만듦, `TUNNEL_TOKEN=`·`PUBLIC_URL=` 두 줄, 시험용 터널 `check.<도메인>`의 값)를 `set -a; . ./check-tunnel.vars; set +a`로 셸에 읽고, 그 셸에서는 앞머리의 `TUNNEL_TOKEN= PUBLIC_URL=`를 빼고 compose를 부른다. 값을 출력하지 않는다. 확인 항목: 휴대폰 접속·로그인, 로그인 응답 `Set-Cookie`에 `Secure`, `restart tunnel` 뒤 같은 주소로 접속, 상태 화면 "연결됨". 끝나면 tunnel도 함께 내린다.
  이유: 본 env 파일에 시험 토큰을 두면 본 Docker가 다음 재시작 때 시험 터널에 붙는다.
- `.gitignore`에 `check-tunnel.vars`를 더한다.
- `deploy` 스킬: 주소 확인 명령 옆에 "`PUBLIC_URL`을 쓰면 상태 화면의 고정 주소로 확인"을 더하고, "tunnel은 다시 만들어지지 않는다"는 문장에 "tunnel 서비스의 명령·환경이 바뀐 배포(15단계 첫 배포 등)에서는 다시 만들어지고, 임시 주소 모드면 주소가 바뀐다"를 덧붙인다.
- `release.sh` 확인은 python 부분을 작업 폴더에 복사해 출력 경로만 작업 폴더로 바꿔 돌린다. `release-dist/`는 건드리지 않는다. 변환이 깨지면 고치지 말고 상위로(`scripts/`는 범위 밖).

## 체크포인트 (각각 검사 통과 후 커밋)
1. `README.md`·`release/INSTALL.md` 고정 주소 안내 → `/check 빠르게`(서식) 통과
2. `real-check`·`deploy` 스킬, `.gitignore` → 셸 빈 값 덮어쓰기 `config` 결과와 `release.sh` 변환 결과를 `handoff.md`에 기록

## 제약
- `.env`는 읽지 않는다. Bash 명령과 가짜 값 파일 이름에 `.env`를 넣지 않는다(hook이 막음).
- 저장소 루트에서 `--env-file` 없이 `docker compose config`를 돌리지 않는다. 본 Docker(프로젝트 `ripple`)와 시험 Docker에 `config` 밖의 compose 명령을 내리지 않는다.
- `docs/`는 고치지 않는다. 명세에 고칠 것이 보이면 `handoff.md`에 적는다.

## 알려진 함정
- 셸에서 빈 값으로 둔 변수(`TUNNEL_TOKEN=`)가 compose에서 env 파일 값을 덮는지는 가정이다. `config` 결과로 확인하고, 덮지 않으면 상위로.
- 리뷰(2026-11-07, tunnel-mode): 토큰이 없어도 tunnel 컨테이너에 `TUNNEL_TOKEN=""`(빈 값)이 넘어가고, 토큰이 있으면 `--url http://web:3000`과 `run`이 함께 붙는다. 둘 다 실제 컨테이너로 확인하지 않았으니 `real-check` 확인 항목에 "토큰 비움에서 임시 주소가 뜬다"와 "고정 주소 확인 때 tunnel 로그에 ingress 경고·오류가 없다"를 넣는다.
- 리뷰(2026-11-07, tunnel-mode): 상태 화면 연결 글자는 "연결됨(연결 N개)"·"Cloudflare와 연결되지 않았습니다"·"터널에 닿지 못했습니다: …"·"PUBLIC_URL 형식이 다릅니다(https://호스트 꼴)"다. 안내·문제 해결 표는 이 글자 그대로 쓴다.

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고) + 셸 빈 값이 env 파일 값을 덮지 못할 때, `release.sh` 변환이 새 compose에서 깨질 때, 공식 문서의 절차가 토큰 방식과 다를 때
