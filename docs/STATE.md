# 지금 상태

> 읽는 때: 세션을 시작할 때 맨 먼저. 지금 하는 일, 답 대기, 다음 할 일만 둔다(화면 한 장 안).
> 이 문서는 덧붙이지 않고 통째로 다시 쓴다(agentflow review가 작업 통과 때, `/handoff`가 단계 밖 작업 뒤). 끝난 일의 자세한 기록은 [DECISIONS.md](DECISIONS.md) 3장, 단계 표·작업 방법은 [DEVELOPMENT.md](DEVELOPMENT.md)에 둔다. 실제와 다르면 `git log`와 `git status`를 먼저 본다.

마지막 갱신: 2026-11-14 (한국 시간), 18단계 `article-classify`의 `display-rule` 통과(브랜치 `af/article-classify`)

## 다음 할 일

- **18단계 `article-classify`: `measure`·`schema`·`market-alias`·`relink-7d`·`stock-subject`·`display-rule` 통과. 다음은 `article-nature`(`/agentflow:run article-classify`로 이어 감).** 종목 평가 요청에 예/아니오 질문 `bystander`(`type` `"noul"`)가 붙어 확률이 `evaluations.bystander`에 저장되고, `display-rule`로 숨김 규칙 하나(`evaluate/run.rs` `NOT_HIDDEN_SQL`)가 타임라인·토픽 카드·토픽 기사 수·키워드 통과 수·통계 낱말에 함께 걸린다: 종목 항목 평가의 `bystander` ≥ 0.5이면 숨기고, `bystander`가 없는 지난 평가는 같은 기사의 시장 항목 관련도가 종목 관련도보다 크면 숨긴다(시장·주제 항목은 받지 않음, 평가 회차 요약 `relevant`는 그대로). 성격 숨김(사진영상·뉴스 모음)은 `article-nature`가 같은 조각에 더한다. 결정은 DECISIONS.md 1장 "기사 분류 개선", "18단계 `article-classify` 계획 답", "18단계 계획 때 정한 세부", "18단계 measure·schema·market-alias·relink-7d·stock-subject·display-rule 구현 세부", 수치는 2장 "기사 분류 측정". `stage-check`는 최대 $0.15 승인됨. 타임라인 조회 속도는 재지 않았다(stage-check에서 실제 DB로 볼 것).
- **`pnpm format:check`**: measure 작업이 커밋한 `work/article-classify/measure/first-response.json`이 prettier 서식이 아니어서 실패한다고 기록했으나, 2026-11-14 stock-subject 리뷰의 `/check`에서는 통과했다(원인 다시 보지 않음). 단계 끝 `close`에서 `work/`가 보관소로 옮겨지면 사라진다.

## 답 대기

- **18단계 measure에서 남긴 질문**: `[자막뉴스]` 4건은 `media` 0.19~0.30으로 숨겨지지 않는다(성격 질문 문구를 바꿀지, 그대로 둘지). 뉴스 모음 `roundup`은 0.52·0.56으로 기준 근처. `article-nature` 전에 볼 것.
- **기사 분류 2-5 나머지**: 젠슨 황 사임 뒤 본인 심경·주변 반응·다른 경영자 비교 기사(영향도 6.9~7.7로 보임)가 왜 엔비디아에 붙는지. 사용자 "지금 그대로, 나중에 수정". 사례는 DECISIONS 2장 "기사 분류 조사".
- **16단계(작음): 손님이 비밀번호 창으로 권한을 얻은 직후 화면.** A(추천) 그대로 둔다(새로 고치면 맞아짐) / B 비밀번호가 맞으면 원래 요청 뒤 화면을 새로 고친다. DECISIONS.md 1장 "16단계 unlock-ui 구현 세부" ⑤.
- **17단계에서 뺀 것(답 11A·12A)**: 네이버 뉴스 검색 API 등 수집 보강(I9), 토픽 갱신 모델 교체·토픽 수·간격·목표 금액. 17단계 배포 뒤 비용을 다시 재고 묻는다.
- **13단계 주가 이력 보관 기간**: 지금 무기한(한국 종목 하나에 하루 약 725줄). 아직 묻지 않음.

## 남은 확인

- 17단계 단계 끝 확인에서 넘어온 것: ① 회차 887에서 종목 키워드 연결 278건이 전부 `gated`(고유어가 너무 좁은지 본 DB의 `article_matches.gated` 비율로 다시 볼 것) ② 시장 항목 통과 수가 직전보다 적었음(한국 9 vs 40, 미국 14 vs 31, 조건이 달라 비교 불확실) ③ Jev 호출 수·토픽 갱신 추론 토큰은 기록이 없어 못 잼. 설정(영향도 기준 5, 키워드 재생성 주 1회)은 2026-11-13 원격 DB에서 반영된 것을 확인함.
- 엔비디아 이름 연결이 17단계 배포 뒤에만 있다(원인 확인 안 함, 엔비디아는 별칭 없음).
- 원격 본 Docker의 장중 주가 갱신(KIS·Finnhub): `stock_prices`의 최근 `fetched_at`과 상태 화면 "주가" 줄로 본다(아직 보지 않음).
- 13단계: 모의 서버 값이 실전과 같은지(사용자가 증권 앱과 대조), KIS 미국 시세가 실시간인지(미국 장중 KIS `last`와 Finnhub `c` 비교). 14단계: KIS 주가 띠(Rust)를 실제 키로 확인, 12~14단계 휴대폰·PC 화면 확인(사용자 몫).
- 16단계 남은 작은 일: `/status/system`의 DB 상태 확인(`server/status.rs` `get_system_status`)이 동기 `db.with`다(회원 전용·가벼운 읽기라 남김).

## 운영 메모

- 본 Docker는 원격 서버 `blockall@blockall-ss`(`~/Ripple`)에서 돈다(2026-11-09 전환, 2026-11-13 기준 주소 192.168.200.100, `known_hosts` 등록됨). 배포는 `/deploy`, 공개 주소 `https://ripple.anjfgkfkrh.com`. Mac에서는 `-p ripple-check` 없이 `up`하지 않는다. 시험용 Docker(`/real-check`)는 Mac. 같은 터널 토큰·KIS 키를 시험 Docker와 본 Docker에서 함께 쓰지 않는다. 시험 볼륨 `ripple-check_ripple-data`는 `down -v`로 안 지워질 수 있어 `docker volume rm`으로 확인한다. 본 DB를 읽기만 할 때는 `scripts/analysis/article-classify/README.md` 1번처럼 `ripple.db`와 `-wal`을 함께 받는다.
- 단계: 0~17단계 완료·배포(17단계 2026-11-11, 기록은 보관소 `../Ripple-archive/keyword-topic/`; 16단계 2026-11-08, `separate-role/`). v0.3.0 릴리스(2026-11-07), v0.2.0 이하에서는 업데이트할 수 없음(DEVELOPMENT.md 7장).
- 사용자가 할 일(개발 환경): ① `sudo chown -R "$(whoami)":staff ~/.npm`(playwright MCP) ② GitHub 토큰을 `GITHUB_PERSONAL_ACCESS_TOKEN`으로 셸 환경에 넣기 ③ 새 세션에서 `/mcp`로 확인.
- 지난 진행 기록(단계별 체크리스트·배포 기록)은 DECISIONS.md 3장.
