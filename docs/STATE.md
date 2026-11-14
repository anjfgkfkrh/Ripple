# 지금 상태

> 읽는 때: 세션을 시작할 때 맨 먼저. 지금 하는 일, 답 대기, 다음 할 일만 둔다(화면 한 장 안).
> 이 문서는 덧붙이지 않고 통째로 다시 쓴다(agentflow review가 작업 통과 때, `/handoff`가 단계 밖 작업 뒤). 끝난 일의 자세한 기록은 [DECISIONS.md](DECISIONS.md) 3장, 단계 표·작업 방법은 [DEVELOPMENT.md](DEVELOPMENT.md)에 둔다. 실제와 다르면 `git log`와 `git status`를 먼저 본다.

마지막 갱신: 2026-11-14 (한국 시간), 18단계 `article-classify`의 마지막 작업 `stage-check` 통과(브랜치 `af/article-classify`)

## 다음 할 일

- **18단계 `article-classify`: 작업 9개 모두 통과. 사용자가 단계 결과(아래 stage-check 수치·캡처)를 확인하고 승인하면 `/agentflow:close article-classify` → main 합치기(합치기 전 `node scripts/check-secrets.mjs`) → `/deploy`.** 종목 평가에 예/아니오 질문 `bystander`, 숨김 규칙 하나(`evaluate/run.rs` `NOT_HIDDEN_SQL` + 성격 숨김 `not_nature_hidden_sql()`)가 타임라인·토픽 카드·토픽 기사 수·키워드 통과 수·통계 낱말에 함께 걸린다. 평가 회차 끝에 화면 기준을 넘은 성격 없는 7일 안 기사마다 Jev 별도 요청으로 성격을 묻고(같은 소식 복사, `purpose = article_nature`), `media`·`roundup` ≥ 0.5 기사는 숨긴다. 타임라인에 "사실"·"해석·전망·기타" 두 탭(`?nature=view`)과 제목 앞 성격 태그. worker 첫 시작 때 최근 7일 기사를 시장 항목에 다시 건다(`classify.marketRelinkedAt`). 결정은 DECISIONS.md 1장 "기사 분류 개선"·"18단계 `article-classify` 계획 답"·"18단계 계획 때 정한 세부"·"18단계 measure~nature-tabs 구현 세부", 수치는 2장 "기사 분류 측정"·"18단계 단계 끝 실제 확인".
- **stage-check 요약(시험 Docker, 실제 AI $0.0604)**: 다시 걸기 새 연결 667, 평가 Jev 호출 842(회차 한도 200으로 4회차), 종목 새 평가 87 중 `bystander` ≥ 0.5 39(화면 기준 넘던 17), 성격 물음 1,331·복사 363·실패 0, 숨김 29(사진영상 2·뉴스 모음 27). 표본 보이는 수 ① 69→50, ② 8→4, ④ 96→92. 배포 직후 본 DB에서 다시 걸기·7일 성격 묻기가 비슷한 양(약 $0.06)으로 몇 회차 돈다. 캡처는 `work/article-classify/stage-check/*.png`(close 뒤 보관소).
- **`pnpm format:check`**: measure가 커밋한 `work/article-classify/measure/first-response.json` 서식 문제 기록이 있었으나 이후 `/check`는 통과. close로 `work/`가 옮겨지면 사라진다.

## 답 대기

- **18단계 measure에서 남긴 질문**: `[자막뉴스]` 4건은 `media` 0.19~0.30으로 숨겨지지 않는다(성격 질문 문구를 바꿀지, 그대로 둘지). 뉴스 모음 `roundup`은 0.52·0.56으로 기준 근처. `article-nature`는 measure 때 글자 그대로 구현됨(문구 바꾸지 않음). stage-check에서 7일 기사 중 사진영상으로 숨겨진 것은 2건뿐(뉴스 모음 27). 단계 확인 때 함께 볼 것.
- **기사 분류 2-5 나머지**: 젠슨 황 사임 뒤 본인 심경·주변 반응·다른 경영자 비교 기사(영향도 6.9~7.7로 보임)가 왜 엔비디아에 붙는지. 사용자 "지금 그대로, 나중에 수정". 사례는 DECISIONS 2장 "기사 분류 조사".
- **16단계(작음): 손님이 비밀번호 창으로 권한을 얻은 직후 화면.** A(추천) 그대로 둔다(새로 고치면 맞아짐) / B 비밀번호가 맞으면 원래 요청 뒤 화면을 새로 고친다. DECISIONS.md 1장 "16단계 unlock-ui 구현 세부" ⑤.
- **17단계에서 뺀 것(답 11A·12A)**: 네이버 뉴스 검색 API 등 수집 보강(I9), 토픽 갱신 모델 교체·토픽 수·간격·목표 금액. 17단계 배포 뒤 비용을 다시 재고 묻는다.
- **13단계 주가 이력 보관 기간**: 지금 무기한(한국 종목 하나에 하루 약 725줄). 아직 묻지 않음.

## 남은 확인

- 17단계 단계 끝 확인에서 넘어온 것: ① 회차 887에서 종목 키워드 연결 278건이 전부 `gated`(고유어가 너무 좁은지 본 DB의 `article_matches.gated` 비율로 다시 볼 것) ② 시장 항목 통과 수가 직전보다 적었음(한국 9 vs 40, 미국 14 vs 31, 조건이 달라 비교 불확실) ③ Jev 호출 수·토픽 갱신 추론 토큰은 기록이 없어 못 잼. 설정(영향도 기준 5, 키워드 재생성 주 1회)은 2026-11-13 원격 DB에서 반영된 것을 확인함.
- 엔비디아 이름 연결이 17단계 배포 뒤에만 있다(원인 확인 안 함, 엔비디아는 별칭 없음).
- 원격 본 Docker의 장중 주가 갱신(KIS·Finnhub): `stock_prices`의 최근 `fetched_at`과 상태 화면 "주가" 줄로 본다(아직 보지 않음).
- 13단계: 모의 서버 값이 실전과 같은지(사용자가 증권 앱과 대조), KIS 미국 시세가 실시간인지(미국 장중 KIS `last`와 Finnhub `c` 비교). 14단계: KIS 주가 띠(Rust)를 실제 키로 확인, 12~14단계 휴대폰·PC 화면 확인(사용자 몫).
- 18단계: 타임라인 탭 SQL 조회 속도를 실제 DB로 재지 않았다(stage-check에서도 안 잼). nature-tabs E2E는 DB에 성격 값을 직접 넣어 가짜 TypeSafe 성격 답 경로를 타지 않는다.
- 16단계 남은 작은 일: `/status/system`의 DB 상태 확인(`server/status.rs` `get_system_status`)이 동기 `db.with`다(회원 전용·가벼운 읽기라 남김).

## 운영 메모

- 본 Docker는 원격 서버 `blockall@blockall-ss`(`~/Ripple`)에서 돈다(2026-11-09 전환, 2026-11-13 기준 주소 192.168.200.100, `known_hosts` 등록됨). 배포는 `/deploy`, 공개 주소 `https://ripple.anjfgkfkrh.com`. Mac에서는 `-p ripple-check` 없이 `up`하지 않는다. 시험용 Docker(`/real-check`)는 Mac. 같은 터널 토큰·KIS 키를 시험 Docker와 본 Docker에서 함께 쓰지 않는다. 시험 볼륨 `ripple-check_ripple-data`는 `down -v`로 안 지워질 수 있어 `docker volume rm`으로 확인한다. 본 DB를 읽기만 할 때는 `scripts/analysis/article-classify/README.md` 1번처럼 `ripple.db`와 `-wal`을 함께 받는다.
- 단계: 0~17단계 완료·배포(17단계 2026-11-11, 기록은 보관소 `../Ripple-archive/keyword-topic/`; 16단계 2026-11-08, `separate-role/`). v0.3.0 릴리스(2026-11-07), v0.2.0 이하에서는 업데이트할 수 없음(DEVELOPMENT.md 7장).
- 사용자가 할 일(개발 환경): ① `sudo chown -R "$(whoami)":staff ~/.npm`(playwright MCP) ② GitHub 토큰을 `GITHUB_PERSONAL_ACCESS_TOKEN`으로 셸 환경에 넣기 ③ 새 세션에서 `/mcp`로 확인.
- 지난 진행 기록(단계별 체크리스트·배포 기록)은 DECISIONS.md 3장.
