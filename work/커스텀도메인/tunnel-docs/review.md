# tunnel-docs 리뷰
결과: 통과
범위 밖 무기록: 0

## 범위 확인
- 검사 명령 재실행: 인계 숫자와 일치. format·lint·typecheck 0, Vitest 7 통과, `cargo fmt --check` 차이 0. 인계가 돌리지 않은 clippy(0)·`cargo test`(601 통과 0 실패)도 돌림(Rust 변경 없음). 셸 빈 값 덮어쓰기 `config`도 가짜 값 파일로 다시 돌림: 앞머리 있으면 `run` 없음·`TUNNEL_TOKEN: ""`·`PUBLIC_URL: ""`, 없으면 `run` 붙음. `git check-ignore`로 `check-tunnel.vars` 제외 확인.
- 범위 밖 판단:
  - 없음(바뀐 파일은 모두 수정 가능 범위 안이거나 PLAN.md·작업 폴더).

## Opus 확인
- 브리프에 없던 결정이 인계에 빠짐없이 있는가: 예. 공식 문서 주소·날짜·메뉴 이름, 서비스 종류 칸이 없는 것, hook 때문에 "환경 변수 파일"·"설정 파일"로 쓴 것, 문제 해결 표 줄, 작업 폴더에 남긴 파일이 모두 있음.
- 테스트 기대값을 바꾸거나 빼서 통과시키지 않았는가: 예. 시험 파일 변경 없음.
- 제약과 프로젝트 규칙을 지켰는가: 예. 사용자 도메인은 `<도메인>`·`example.com`으로만 씀, `docs/`·`scripts/`·`release-dist/` 안 고침. 상태 화면 글자는 `rust/ripple/src/web/status.rs:476-489`와 그대로 같음. real-check의 compose 명령은 4-1장(의도된 예외) 밖 모두 앞머리가 붙음.
- 인계가 실제 변경과 맞는가: 대체로 예. 인계의 `커밋: d8d269d`는 실제 커밋(`063a7e6`·`44225bb`)과 다름(내용은 맞음).

## 리뷰가 직접 고친 것(브리프 빈틈, 커밋 `f1a7b30`)
- README "시험용 Docker"의 띄우기·내리기 명령에 `TUNNEL_TOKEN= PUBLIC_URL=`가 없었다. 이 명령은 tunnel까지 띄우므로, 사람이 본 env 파일에 토큰을 넣은 뒤 따르면 결정 ⑧이 막으려던 접속 분산이 생긴다. 브리프가 앞머리를 `real-check`에만 지정한 빈틈이라 리뷰가 붙였다(DEVELOPMENT 4장도 같게).
- 토큰 위치 문구: 공식 문서는 "운영 체제를 고르고 설치 명령을 복사"라고만 하고 토큰을 말하지 않는다(리뷰도 다시 읽음). 운영 체제마다 명령 꼴이 달라 `--token`이 없을 수 있어 "Docker를 고르면 나오는 명령의 `--token` 뒤 문자열"로 고쳤다. 실제 대시보드로는 확인하지 못해 STATE의 사람이 할 일에 넣었다.
- README·INSTALL 환경 변수 표에 `TUNNEL_TOKEN`·`PUBLIC_URL` 선택 행을 더했다.

## 남은 위험 (실제 확인 몫)
- 빈 `TUNNEL_TOKEN`에서 Quick Tunnel이 뜨는지, `--url`+`run`에서 ingress 경고가 없는지는 아직 실제 컨테이너로 보지 않았다(`/real-check` 3장·4-1장 확인 항목으로 들어 있음).
- `PUBLIC_URL` 끝 `/`는 코드가 허용하지만(`tunnel.rs:98`) 안내는 "끝에 `/` 없이"라고 쓴다. 더 엄격한 안내라 문제 없음.

## 반영한 것 (통과일 때)
- 영구 문서: DECISIONS 1장에 "15단계 tunnel-docs 안내 세부" 줄(`근거: 커스텀도메인/tunnel-docs`). DEVELOPMENT 4장 시험용 Docker 띄우기·내리기에 앞머리와 `check-tunnel.vars` 관례. STATE.md 다시 씀(두 작업 통과, 사람이 할 일 1~3, 미확인 항목). SPEC은 tunnel-mode 리뷰 때 이미 고쳐져 더 고칠 것 없음.
- 다음 브리프 확인: 다음 작업 없음(단계의 마지막 작업).
