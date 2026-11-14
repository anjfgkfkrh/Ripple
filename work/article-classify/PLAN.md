# article-classify 계획
목표: 지수·다른 대상이 주어인 기사를 종목에서 빼 시장으로 보내고, 보이는 기사를 성격(사실·해석·전망·기타) 태그와 두 탭으로 나눈다

> 상태 값: 대기 / 진행 / 질문 / 되돌리기 / 통과 / 반려 / 보고 / 사람
> 시도: 작업을 worker에게 맡긴 횟수(숫자)

| 작업 | 목표 | 완료 조건 | 모델/effort | 선행 | 상태 | 시도 |
|---|---|---|---|---|---|---|
| measure | 무료: 보이는 기사 수·지난 평가 규칙 결과·별칭 걸림 수를 세고, 유료: Jev 예/아니오 응답 모양과 성격·종목 질문 일관성을 잰다 | 인계에 무료 수치 표, 첫 유료 응답 원문, 일관성 표, 비용($0.05 이하) | Sonnet / medium | 없음 | 진행 | 1 |
| schema | 마이그레이션 0012(`articles` 성격 6칸·`nature_at`, `evaluations.bystander`)와 읽기 구조 | `migrate_tests`·`schema_tests` 새 시험 통과, Rust 검사 전체 통과 | Sonnet / medium | measure 통과 | 대기 | 0 |
| market-alias | 한국·미국 증시를 코드 고정 별칭 구절로 제목+요약에서 건다 | `match_tests`·`collect/run_tests` 새 시험 통과, `/check` 전체 통과 | Sonnet / medium | schema 통과 | 대기 | 0 |
| relink-7d | worker 시작 때 최근 7일 저장 기사를 시장 항목에 한 번 다시 건다 | `backfill_tests` 새 시험 통과, `/check` 전체 통과 | Sonnet / medium | market-alias 통과 | 대기 | 0 |
| stock-subject | 종목 평가에 예/아니오 질문 `bystander`를 더해 해석·저장·복사 | `questions_tests`·`typesafe_tests`·`evaluate/run_tests` 새 시험 통과, `/check` 전체 통과 | Sonnet / medium | measure 통과, schema 통과 | 사람 | 0 |
| display-rule | 숨김 규칙 하나(새 질문·지난 평가 규칙)를 타임라인·토픽 수·키워드 통과·통계 낱말에 적용 | `timeline/read_tests`·`store` 시험·`performance_tests`·`stat_words` 시험 새 시험 통과, `/check` 전체 통과 | Sonnet / medium | stock-subject 통과 | 대기 | 0 |
| article-nature | 보이는 기사의 성격을 Jev 별도 요청으로 묻고 저장·복사, 사진영상·뉴스 모음 숨김 | `questions_tests`·`typesafe_tests`·`evaluate` 시험 새 시험 통과, `/check` 전체 통과 | Sonnet / medium | display-rule 통과 | 대기 | 0 |
| nature-tabs | 제목 앞 성격 태그와 사실 / 해석·전망·기타 탭 | `timeline/read_tests`·`web/timeline_tests` 새 시험, E2E `timeline.spec.ts` 새 시험 통과, `/check` 전체 통과, 휴대폰·PC 캡처 | Sonnet / medium | article-nature 통과 | 대기 | 0 |
| stage-check | 시험 Docker에서 실제 AI로 다시 걸기·평가·성격 묻기를 돌리고 표본 ①②④를 다시 센다 | 인계에 항목별 수치·비용($0.15 이하)·화면 캡처, 시험 Docker 내림 | Sonnet / medium | 앞 작업 모두 통과 | 대기 | 0 |

## 사람이 승인한 큰 결정
- DECISIONS.md 1장 "기사 분류 개선"(2026-11-13, ①~⑱): 주어·성격 두 축, 시장 고정 별칭(제목+요약), 최근 7일 다시 걸고 평가, 종목 예/아니오 질문, 지난 기사는 표시 규칙, 성격 태그 여럿·두 탭, 사진영상·뉴스 모음 숨김, 일관성 유료 측정 최대 $0.05.
- DECISIONS.md 1장 "18단계 `article-classify` 계획 답"(2026-11-14, "1A~12A"): 보이는 기사마다 성격 별도 요청·같은 소식 복사, 확률 그대로 저장·기준 0.5, 종목 질문 영어 문구("or in passing" 더함), 탭 모양·`?nature=view`·성격 없는 기사는 사실 탭, 지난 평가는 같은 기사 시장 관련도 > 종목 관련도면 숨김, 미국 별칭 손보기, 숨김은 한 규칙으로 모든 통과 셈에, 7일 보이는 기사 성격 한 번 묻기, worker 시작 때 다시 걸기, stage-check $0.15 승인.
- DECISIONS.md 1장 "18단계 계획 때 정한 세부"(구현 중 정함, 계획 보고에 알림).
- 범위 밖: 2-5(젠슨 황 사임 뒤 본인 심경·주변 반응 기사)는 이 단계에서 고치지 않는다.
- 사전 조건: `measure`·`stage-check` 전에 사용자가 원격 최신 백업을 `.check-restore/ripple.db`로 받아 둔다(원격 조회는 권한에서 막힐 수 있음).
- 사람 확인(`stock-subject` 상태 `사람`): `measure`가 통과하면 상위가 질문 문구·기준값을 `stock-subject`·`article-nature` 브리프에 확정해 넣고, 지난 평가 규칙 결과와 7일 성격 묻기 비용을 사용자에게 알린다. 사람이 `stock-subject` 상태를 `대기`로 되돌리면 뒤 작업이 이어진다(`display-rule`·`article-nature`·`nature-tabs`·`stage-check`는 선행으로 함께 기다린다).
