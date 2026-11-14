# 지금 상태

> 읽는 때: 세션을 시작할 때 맨 먼저. 지금 하는 일, 답 대기, 다음 할 일만 둔다(화면 한 장 안).
> 이 문서는 덧붙이지 않고 통째로 다시 쓴다(agentflow review가 작업 통과 때, `/handoff`가 단계 밖 작업 뒤). 끝난 일의 자세한 기록은 [DECISIONS.md](DECISIONS.md) 3장, 단계 표·작업 방법은 [DEVELOPMENT.md](DEVELOPMENT.md)에 둔다. 실제와 다르면 `git log`와 `git status`를 먼저 본다.

마지막 갱신: 2026-11-14 (한국 시간), 18단계 `article-classify`의 `relink-7d` 통과(브랜치 `af/article-classify`)

## 다음 할 일

- **18단계 `article-classify`: `measure`·`schema`·`market-alias`·`relink-7d` 통과. `stock-subject`는 사람 확인을 기다린다(남은 작업은 모두 그 뒤).** 아래 "답 대기" 첫 항목을 보고 이의가 없으면 `work/article-classify/PLAN.md`의 `stock-subject` 상태를 `대기`로 되돌린 뒤 `/agentflow:run article-classify`. `market-alias`로 시장 항목(한국·미국 증시)이 코드 고정 별칭 구절(제목+요약)로 이름 연결되고, `relink-7d`로 worker가 처음 시작할 때 한 번 최근 7일 저장 기사를 시장 항목에 다시 건다(설정 `classify.marketRelinkedAt`, AI 없음, 평가는 다음 수집 회차). 결정은 DECISIONS.md 1장 "기사 분류 개선", "18단계 `article-classify` 계획 답", "18단계 계획 때 정한 세부", "18단계 measure·schema·market-alias·relink-7d 구현 세부", 수치는 2장 "기사 분류 측정". `stage-check`는 최대 $0.15 승인됨.
- **`pnpm format:check`가 실패한다**: `work/article-classify/measure/first-response.json`(measure 작업이 커밋한 응답 원문)이 prettier 서식이 아니다. 코드와 무관해 작업들은 통과로 보고 있다. `work/`를 `.prettierignore`에 넣거나 파일을 서식화할지는 상위·사람이 정한다(단계 끝 `close`에서 보관소로 옮기면 사라짐).

## 답 대기

- **18단계 measure 결과 확인(`stock-subject`를 `대기`로 되돌릴지)**: 질문 문구·기준 0.5는 계획대로 두고 브리프에 확정함(예/아니오 `type`은 `"noul"`). 사람이 볼 것 ① 지난 평가 규칙(같은 기사 시장 관련도 > 종목 관련도면 숨김)은 화면 기준을 넘는 종목 평가 2,106건 중 79건을 숨긴다. 그중 23건은 표본 ④(정상 기사가 많은 묶음), 20건은 표본 밖(목록 DECISIONS 2장·measure 인계 c). 계획 답 ⑤ "정상 기사가 많이 숨으면 멈추고 다시 묻는다"의 판단이 필요 ② 7일 성격 한 번 묻기 약 $0.043~0.048(1,632건), 이후 하루 약 $0.007 ③ `bystander` ≥ 0.5: ① 15/15, ④ 4/10, 회사 기사 2/15(s38·s39, 다른 회사 영향 기사), 형식 1/10 ④ `[자막뉴스]` 4건은 `media` 0.19~0.30으로 숨겨지지 않는다(질문 문구를 바꿀지, 그대로 둘지). 뉴스 모음 `roundup`은 0.52·0.56으로 기준 근처.
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
