# 지금 상태

> 읽는 때: 세션을 시작할 때 맨 먼저. 지금 하는 일, 답 대기, 다음 할 일만 둔다(화면 한 장 안).
> 이 문서는 덧붙이지 않고 통째로 다시 쓴다(agentflow review가 작업 통과 때, `/handoff`가 단계 밖 작업 뒤). 끝난 일의 자세한 기록은 [DECISIONS.md](DECISIONS.md) 3장, 단계 표·작업 방법은 [DEVELOPMENT.md](DEVELOPMENT.md)에 둔다. 실제와 다르면 `git log`와 `git status`를 먼저 본다.

마지막 갱신: 2026-11-11 (한국 시간), 17단계 `keyword-topic` main 합치기·푸시·본 Docker 배포(16:02)

## 다음 할 일

- **17단계 키워드 트리·토픽 고도화(`keyword-topic`): 작업 10개 모두 리뷰 통과, 사람의 단계 결과 확인 대기.** 브랜치 `af/keyword-topic`(main에 합치기 전, 합칠 때 `node scripts/check-secrets.mjs`). 작업 표는 `work/keyword-topic/PLAN.md`, 각 작업 세부는 DECISIONS 1장 "17단계 … 구현 세부", 결정은 1장 "키워드 트리·토픽 고도화" 세 줄·"17단계 `keyword-topic` 계획 답"·"17단계 계획 때 정한 세부"·"17단계 추론 토큰"·"17단계 토픽 통과 건수"·"17단계 tree-screen 상태 화면 문구". 승인되면 `/agentflow:close keyword-topic`, 그 뒤 main 합치기·`/deploy`는 사람 요청 때.
  - 단계 끝 실제 확인(`stage-check`, 2장 "17단계 단계 끝 실제 확인"): $0.0212, 앱 오류 0. 전체 생성 별칭·고유어 정상(버린 별칭 0), 정기 점검이 옛 시장 트리를 상한으로 줄임, company 토픽 1개 생김.
  - **단계 결과 보고 때 사람이 볼 것**: ① 회차 887에서 종목 키워드 연결 278건이 **전부** `gated`(통과 0). 결정 ⑦대로 저장만 하고 평가하지 않은 것이지만, 고유어 15개가 제목에 하나도 없었는지·고유어가 너무 좁은지는 시험 DB를 지워 확인하지 못했다. 배포 뒤 본 DB에서 `article_matches.gated` 비율과 종목 키워드 경로를 다시 보거나, 시험 Docker로 다시 잴지 정한다. ② 시장 항목 통과 수가 직전 기간보다 적다(한국 9 vs 40, 미국 14 vs 31, 영향도 ≥5). 질문이 바뀌었고(관련도 한·미 합침) 직전 쪽이 4.3시간 비어 같은 조건이 아니다. ③ Jev 호출 수와 토픽 갱신 추론 토큰은 기록이 없어 재지 못함.
  - 배포 뒤 사용자 몫: 설정 화면에서 영향도 표시 기준 5, 키워드 재생성 주기 주 1회로 바꾸기(저장된 값은 코드가 덮지 않음, 답 3B·4A. 시험 DB에서도 6·하루 1회로 남아 있었음). 0011은 배포 때 종목을 `pending`으로 돌려 worker 시작 때 전체 생성을 한 번 한다(답 8A).

## 답 대기

- **16단계(작음): 손님이 비밀번호 창으로 권한을 얻은 직후 화면.** A(추천) 그대로 둔다(새로 고치면 맞아짐) / B 비밀번호가 맞으면 원래 요청 뒤 화면을 새로 고친다. DECISIONS.md 1장 "16단계 unlock-ui 구현 세부" ⑤.
- **17단계에서 뺀 것(답 11A·12A)**: 네이버 뉴스 검색 API 등 수집 보강(I9), 토픽 갱신 모델 교체·토픽 수·간격·목표 금액. 17단계 배포 뒤 비용을 다시 재고 묻는다.
- **13단계 주가 이력 보관 기간**: 지금 무기한(한국 종목 하나에 하루 약 725줄). 아직 묻지 않음.

## 남은 확인

- 원격 본 Docker의 장중 주가 갱신(KIS·Finnhub): `stock_prices`의 최근 `fetched_at`과 상태 화면 "주가" 줄로 본다(아직 보지 않음).
- 13단계: 모의 서버 값이 실전과 같은지(사용자가 증권 앱과 대조), KIS 미국 시세가 실시간인지(미국 장중 KIS `last`와 Finnhub `c` 비교). 14단계: KIS 주가 띠(Rust)를 실제 키로 확인, 12~14단계 휴대폰·PC 화면 확인(사용자 몫).
- 16단계 남은 작은 일: `/status/system`의 DB 상태 확인(`server/status.rs` `get_system_status`)이 동기 `db.with`다(회원 전용·가벼운 읽기라 남김).

## 운영 메모

- 본 Docker는 원격 서버 `blockall@blockall-ss`(`~/Ripple`)에서 돈다(2026-11-09 전환). 배포는 `/deploy`, 공개 주소 `https://ripple.anjfgkfkrh.com`. Mac에서는 `-p ripple-check` 없이 `up`하지 않는다. 시험용 Docker(`/real-check`)는 Mac. 같은 터널 토큰·KIS 키를 시험 Docker와 본 Docker에서 함께 쓰지 않는다. 시험 볼륨 `ripple-check_ripple-data`는 `down -v`로 안 지워질 수 있어 `docker volume rm`으로 확인한다. `.check-restore/ripple.db`는 `stage-check` 뒤 지웠다(다시 쓰려면 새로 받는다).
- 단계: 0~17단계 완료·배포(17단계 2026-11-11, 기록은 보관소 `../Ripple-archive/keyword-topic/`; 16단계 2026-11-08, `separate-role/`). v0.3.0 릴리스(2026-11-07), v0.2.0 이하에서는 업데이트할 수 없음(DEVELOPMENT.md 7장).
- 사용자가 할 일(개발 환경): ① `sudo chown -R "$(whoami)":staff ~/.npm`(playwright MCP) ② GitHub 토큰을 `GITHUB_PERSONAL_ACCESS_TOKEN`으로 셸 환경에 넣기 ③ 새 세션에서 `/mcp`로 확인.
- 지난 진행 기록(단계별 체크리스트·배포 기록)은 DECISIONS.md 3장.
