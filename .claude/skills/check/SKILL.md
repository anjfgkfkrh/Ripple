---
name: check
description: 전체 검사(서식·린트·타입·단위·빌드·E2E)를 돌리고 결과 표만 돌려준다. 코드를 바꾼 뒤, 완료를 보고하거나 커밋·배포하기 전에 쓴다. "빠르게"를 붙이면 서식·린트·타입·단위만.
argument-hint: "[빠르게]"
context: fork
agent: general-purpose
background: false
model: sonnet
---

Ripple 저장소 루트에서 아래 검사를 차례로 돌리고 결과만 보고한다. 파일은 고치지 않는다(고치는 것은 부른 쪽이 한다).

인자: `$ARGUMENTS` — "빠르게"면 1~4와 R1~R3만 돈다.

Rust(14단계, `rust/`가 있을 때. 명령은 `rust/` 폴더에서):

- R1. `cargo fmt --check`
- R2. `cargo clippy --all-targets -- -D warnings`
- R3. `cargo test`
- R4. `./build.sh --release`(CSS + 빌드). "빠르게"면 건너뜀

TS(R13에서 TS를 지우기 전까지):

1. `pnpm format:check`
2. `pnpm lint`
3. `pnpm typecheck`
4. `pnpm test`
5. `pnpm build`
6. `pnpm build:worker`
7. `pnpm test:e2e` — 몇 분 걸리므로 명령 시간 제한을 10분으로 준다. 가짜 외부 서비스(4200)와 시험용 worker(4100)·web(3100)을 띄우고 유료 API는 부르지 않는다.

앞 검사가 실패해도 나머지는 돈다. 단, 5가 실패하면 7은 건너뛰고 그렇게 적는다.

보고(한국어, 이 형식만):

| 검사 | 결과 | 숫자 |
|---|---|---|

(Rust 검사 R1~R4도 같은 표에 넣는다.)

- 숫자: 단위·E2E는 출력 요약 줄의 통과·실패·건너뜀 개수와 파일 수, 나머지는 오류·경고 수.
- E2E 건너뜀은 설계상 몇 개 있다(모바일에서 건너뛰는 시험). 지난 보고와 개수가 다르면 따로 적는다.
- 실패가 있으면 표 아래에 검사마다 첫 오류 몇 줄만 붙인다(파일:줄, 메시지, 실패한 테스트 이름). 긴 로그는 붙이지 않는다.
- 돌리지 못한 검사가 있으면 이유와 함께 "돌리지 못함"으로 적는다. 돌리지 않은 것을 통과로 적지 않는다.
