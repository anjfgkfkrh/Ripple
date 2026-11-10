# keyword-topic 계획
목표: 키워드 트리·토픽을 고도화해 AI 토큰을 줄이고(월 약 $4.16 → 약 $1.9) 주가에 큰·중간 영향을 주는 기사를 모은다

> 상태 값: 대기 / 진행 / 질문 / 되돌리기 / 통과 / 반려 / 보고 / 사람
> 시도: 작업을 worker에게 맡긴 횟수(숫자)

| 작업 | 목표 | 완료 조건 | 모델/effort | 선행 | 상태 | 시도 |
|---|---|---|---|---|---|---|
| paid-compare | 숨은 추론 토큰 끄기와 이전 토픽 목록의 통과 건수를 유료로 비교해 수치를 모은다 | 인계에 OpenRouter 문서 근거와 비교 표·비용, 합계 $0.08 이하 | Sonnet / medium | 없음 | 통과 | 2 |
| llm-request | OpenRouter 본문 실패 분류(B1)·한 번 재시도·추론 설정, 결과 줄의 실제 깊이(B2) | `openrouter_tests`·`queue_tests` 새 시험 통과, Rust 검사 전체 통과 | Sonnet / medium | paid-compare 통과 | 통과 | 1 |
| anchor-schema | 마이그레이션 0011(별칭·고유어·지운 별칭·`scope`·`gated`), 저장·읽기, 별칭 검증 | `migrate_tests`·`repository_tests`·`store` 시험 통과, Rust 검사 전체 통과 | Sonnet / medium | 없음 | 통과 | 2 |
| anchor-match | 별칭을 이름처럼 걸고, 종목 키워드 연결은 제목 고유어가 없으면 `gated`로 저장·평가 제외 | `match_tests`·`run_tests`(collect·evaluate)·`backfill_tests` 새 시험 통과, Rust 검사 전체 통과 | Sonnet / medium | anchor-schema 통과 | 통과 | 1 |
| tree-full | 새 전체 생성(깊이 2·상한·별칭·고유어·통계 낱말), Jev 후보 평가 제거 | `tree_tests`·`generate_tests` 새 시험 통과, `/check` 전체 통과 | Sonnet / medium | llm-request 통과, anchor-match 통과 | 통과 | 1 |
| tree-weekly | 키워드 실적, 정기 증분 점검(뺄 것·더할 것), 상한·밀어내기, 매일 06:00 확인 | `generate_tests`·`plan_tests`·`queue_tests` 새 시험 통과, `/check` 전체 통과 | Sonnet / medium | tree-full 통과 | 통과 | 1 |
| tree-screen | 트리 화면 실적 표시, 별칭 보기·지우기, 설정 이름·범위·기본값 | `stocks_tests`·`settings_form_tests` 새 시험, 고친 E2E 통과, `/check` 전체 통과, 화면 캡처 | Sonnet / medium | tree-weekly 통과 | 통과 | 3 |
| eval-merge | 시장·토픽 기사를 기사당 Jev 한 요청으로 평가하고 연결마다 저장, 사용량 나눠 기록 | `questions_tests`·`typesafe_tests`·`evaluate/run_tests` 새 시험 통과, `/check` 전체 통과 | Sonnet / medium | anchor-match 통과 | 통과 | 1 |
| topic-input | 토픽 갱신 입력 줄이기(일본어·같은 소식 묶기)·새 규칙·`scope`·company 처리·통과 기준 바로잡기 | `topics_tests` 새 시험 통과, `/check` 전체 통과 | Sonnet / medium | paid-compare 통과, anchor-schema 통과 | 대기 | 0 |
| stage-check | 시험 Docker에서 실제 AI로 전체 흐름을 확인하고 수치를 모은다 | 인계에 항목별 수치·비용($0.15 이하)·화면 캡처, 시험 Docker 내림 | Sonnet / medium | 앞 작업 모두 통과 | 대기 | 0 |

## 사람이 승인한 큰 결정
- DECISIONS.md 1장 "키워드 트리·토픽 고도화" 세 줄(2026-11-10): 고유어 조건, 기사당 한 요청, 트리 주 1회·깊이 2·상한·증분, 토픽 입력 줄이기·`scope`, 영향도 기준 5, 별칭, 트리 화면 수치.
- DECISIONS.md 1장 "17단계 `keyword-topic` 계획 답"(답 "1A 2A 3B 4A 5A 6AA 7B 8A 9A 10A 11A 12A"): 0011에 칸만 더함, 깊이·주기 설정은 남김(1~2, 기본 주 1회), 영향도 5는 코드 기본값만, 재시도 1번·다음 날 다시, 단계 끝 실제 확인 $0.15 이하 승인·사용량 옮기지 않음, 걸러진 연결은 저장하고 평가만 안 함, 배포 직후 1회 전체 생성, 사용량은 항목 수로 나눔, B2 고침, I9·모델 교체는 뺌.
- DECISIONS.md 1장 "17단계 계획 때 정한 세부"(구현 중 정함, 계획 보고에 알림).
- 사전 조건: `paid-compare`·`stage-check` 전에 사용자가 원격 최신 백업을 `.check-restore/ripple.db`로 받아 둔다(이 세션에서 원격 조회가 권한에서 막힘).
