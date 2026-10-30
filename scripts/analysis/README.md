# 분석 스크립트 (본 DB 읽기 전용)

모두 `docker compose exec -T worker node - < 파일`로 실행한다(DB는 readonly로 연다). 쓰기·AI 호출 없음.

| 파일 | 하는 일 |
|---|---|
| `db-query.sh` | SQL 파일을 읽기 전용으로 돌려 표로 출력 |
| `topic-sim.cjs` | 토픽 24시간 최소 유지를 가정한 시뮬레이션(재생성 흡수, 평균 진행 토픽 수) |
| `topic-sim2.cjs` | 화면에 나온(기준 이상 기사 있는) 토픽 목록과 조건부 유지 시 진행 토픽 수 |
| `topic-sim3.cjs` | 갱신 로그(표준 입력, `토픽 갱신:` 줄)를 제목 수 구간별로 집계 |
| `topic-win.cjs` | 갱신 시각별 지난 갱신 뒤 제목 수·24시간 제목 수·1,500개 상한이 덮는 시간 |
| `topic-endsim.cjs` | 끝난 토픽의 최근 24시간 검색어 적중 건수 |
| `topic-cmp.cjs` | 진행 중 토픽의 24시간 적중 수와 저장된 연결·화면 통과 수 비교 |
| `topic-retention-compare.ts` | **유료** 토픽 유지 개선 비교: 지난 갱신 입력을 옛·새 프롬프트로 다시 돌림(OpenRouter 실제 호출, 회당 약 $0.005) |

`topic-retention-compare.ts` 실행: esbuild로 묶어(`alias: {"@": "<저장소>/src"}`, `external: ["better-sqlite3"]`, `scripts/build-worker.mjs`와 같은 banner) 임시 폴더에 두고, `/real-check` 방식으로 `docker compose -p ripple-check run --rm --no-deps -T -v "$PWD/backups":/restore:ro -v <임시 폴더>:/app/exp:ro -e DB_PATH=/tmp/check.db -e COST_CAP=0.15 worker sh -c 'cp /restore/<백업> /tmp/check.db && node /app/exp/compare.mjs'` (AI 키는 compose가 줌). `ONLY="10-29 06:56"`로 갱신 시각 고르기, `SKIP_OLD=1`, `DUMP_RAW=1`(실패 응답 원문). 끝나면 `down -v`, 사용량을 `topic_refresh_check`로 본 DB에.

날짜·경로가 코드에 박혀 있다(예: 2026-10-23 기준). 다시 쓸 때 고친다.
