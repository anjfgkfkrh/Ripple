# 지금 상태

> 읽는 때: 세션을 시작할 때 맨 먼저. 지금 하는 일, 답 대기, 다음 할 일만 둔다(화면 한 장 안).
> 이 문서는 덧붙이지 않고 통째로 다시 쓴다(agentflow review가 작업 통과 때, `/handoff`가 단계 밖 작업 뒤). 끝난 일의 자세한 기록은 [DECISIONS.md](DECISIONS.md) 3장, 단계 표·작업 방법은 [DEVELOPMENT.md](DEVELOPMENT.md)에 둔다. 실제와 다르면 `git log`와 `git status`를 먼저 본다.

마지막 갱신: 2026-11-10 (한국 시간), 17단계 `keyword-topic` 작업 `tree-weekly` 리뷰 통과

## 다음 할 일

- **17단계 키워드 트리·토픽 고도화(`keyword-topic`): run 진행 중.** 브랜치 `af/keyword-topic`, 작업 10개는 `work/keyword-topic/PLAN.md`(상태는 PLAN 표). 통과: `paid-compare`(OpenRouter 28회 $0.0624), `llm-request`(B1·B2, 추론 끄기 기본), `anchor-schema`(마이그레이션 0011, 별칭 검증), `anchor-match`(별칭은 이름 연결, 고유어 없는 종목 키워드 연결은 `gated`), `tree-full`(LLM 한 번으로 깊이 2·1단계 6·2단계 12 상한 트리와 별칭·고유어를 받아 한 트랜잭션에 저장, 통계 낱말 참고, Jev 후보 평가 없앰, 고유어 0개면 실패, 1장 "17단계 tree-full 구현 세부"), `tree-weekly`(정기 점검은 매일 06:00 확인·주기 시작 전 항목만, 지금 트리에서 뺄 것·더할 것 최대 3개만 받아 반영, 28일 통과 실적으로 지키고 상한 넘으면 마지막 통과가 오래된 것부터 밀어냄, 트리·고유어가 없으면 전체 생성, Rust 시험 698개·E2E 49 통과, 1장 "17단계 tree-weekly 구현 세부"). 각 작업 세부는 1장 "17단계 … 구현 세부".
  - 리뷰가 정한 것(계획 세부 ⑪, 사용자 확인 필요하면 단계 결과 보고 때): DECISIONS 1장 "17단계 추론 토큰"(OpenRouter 요청에 `reasoning: {"enabled": false}` 기본) · "17단계 토픽 통과 건수"(이전 토픽 줄에 `· 통과 M건` 보임). 수치는 2장 "17단계 추론·통과 건수 비교".
  - 다음: `tree-screen`, `eval-merge`, `topic-input`(선행이 모두 끝남). 그 뒤 `stage-check`. 0011은 배포 때 기존 종목을 모두 `pending`으로 돌려 worker 시작 때 전체 생성을 한 번 한다(답 8A). `stage-check`가 다시 `.check-restore/ripple.db`를 쓴다(그 전에 최신 백업으로 바꿔 받을지는 그때 확인). 시험 Docker 확인 때 `validate_aliases`가 버린 별칭 수와 통계 낱말 계산 시간(`feed_headlines` 전체를 읽음, 정기 점검도 항목마다 읽음), 정기 점검 한 번의 시간·비용과 옛 트리가 상한으로 줄어드는 모습을 본다.
  - 결정: DECISIONS.md 1장 "키워드 트리·토픽 고도화" 세 줄, "17단계 `keyword-topic` 계획 답", "17단계 계획 때 정한 세부", 위 두 줄. SPEC 10장 17단계 줄과 FR-3·4·5·6·8·7장 고침.
  - 배포 뒤 사용자 몫: 설정 화면에서 영향도 표시 기준 5, 키워드 재생성 주기 주 1회로 바꾸기(저장된 값은 코드가 덮지 않음, 답 3B·4A).

## 답 대기

- **16단계(작음): 손님이 비밀번호 창으로 권한을 얻은 직후 화면.** A(추천) 그대로 둔다(새로 고치면 맞아짐) / B 비밀번호가 맞으면 원래 요청 뒤 화면을 새로 고친다. DECISIONS.md 1장 "16단계 unlock-ui 구현 세부" ⑤.
- **17단계에서 뺀 것(답 11A·12A)**: 네이버 뉴스 검색 API 등 수집 보강(I9), 토픽 갱신 모델 교체·토픽 수·간격·목표 금액. 17단계 배포 뒤 비용을 다시 재고 묻는다.
- **13단계 주가 이력 보관 기간**: 지금 무기한(한국 종목 하나에 하루 약 725줄). 아직 묻지 않음.

## 남은 확인

- 원격 본 Docker의 장중 주가 갱신(KIS·Finnhub): `stock_prices`의 최근 `fetched_at`과 상태 화면 "주가" 줄로 본다(2026-11-10 장 시작 뒤 확인 예정이었고 이 세션에서는 보지 않음).
- 13단계: 모의 서버 값이 실전과 같은지(사용자가 증권 앱과 대조), KIS 미국 시세가 실시간인지(미국 장중 KIS `last`와 Finnhub `c` 비교). 14단계: KIS 주가 띠(Rust)를 실제 키로 확인, 12~14단계 휴대폰·PC 화면 확인(사용자 몫).
- 16단계 남은 작은 일: `/status/system`의 DB 상태 확인(`server/status.rs` `get_system_status`)이 동기 `db.with`다(회원 전용·가벼운 읽기라 남김).

## 운영 메모

- 본 Docker는 원격 서버 `blockall@blockall-ss`(`~/Ripple`)에서 돈다(2026-11-09 전환). 배포는 `/deploy`, 공개 주소 `https://ripple.anjfgkfkrh.com`. Mac에서는 `-p ripple-check` 없이 `up`하지 않는다. 시험용 Docker(`/real-check`)는 Mac. 같은 터널 토큰·KIS 키를 시험 Docker와 본 Docker에서 함께 쓰지 않는다.
- 단계: 0~16단계 완료·배포(16단계 2026-11-08, 기록은 보관소 `../Ripple-archive/separate-role/`). v0.3.0 릴리스(2026-11-07), v0.2.0 이하에서는 업데이트할 수 없음(DEVELOPMENT.md 7장).
- 사용자가 할 일(개발 환경): ① `sudo chown -R "$(whoami)":staff ~/.npm`(playwright MCP) ② GitHub 토큰을 `GITHUB_PERSONAL_ACCESS_TOKEN`으로 셸 환경에 넣기 ③ 새 세션에서 `/mcp`로 확인.
- 지난 진행 기록(단계별 체크리스트·배포 기록)은 DECISIONS.md 3장.
