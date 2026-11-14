# relink-7d: 최근 7일 기사를 시장 항목에 한 번 다시 건다
모델: Sonnet / effort: medium | 선행: market-alias 통과

## 목표
worker가 시작할 때 한 번, 저장된 기사 중 최근 7일 것을 시장 항목에만 다시 맞춰 새 연결을 저장한다. 끝나면 설정 값에 표시해 다시 돌지 않게 한다. 평가는 다음 수집 회차의 평가 대기로 한다.

## 완료 조건
- `backfill_tests` 새 시험 통과:
  - 최근 7일 기사 중 별칭이 있고 시장 연결이 없던 기사에 시장 항목 연결이 생기고, 7일보다 오래된 기사에는 생기지 않음
  - 이미 그 시장 항목에 걸린 기사(키워드 연결 포함)는 그대로(새 줄 없음)
  - 종목·주제 항목 연결은 생기지 않음
  - 지운 시장 항목(`deleted_at`)에는 걸지 않음
  - 표시가 있으면 두 번째 실행은 아무것도 하지 않음, 처음 실행 뒤 표시가 남음
- worker 시작에서 부르는 곳의 시험(시작 함수 시험이 없으면 다시 걸기 함수를 부르는 작은 함수의 시험)
- `/check` 전체 통과(E2E 포함. E2E 가짜 기사에 별칭 낱말이 있어 연결이 늘어 깨지면 시험 준비를 고친다)

## 수정 가능 범위
- `rust/ripple/src/server/collect/backfill.rs`
- `rust/ripple/src/server/collect/backfill_tests.rs`
- `rust/ripple/src/server/settings.rs`(표시 설정 하나)
- `rust/ripple/src/worker/index.rs`(시작 때 한 번 부르기와 결과 로그 한 줄)
- `e2e/timeline.spec.ts`(worker 시작 때 다시 걸기로 E2E가 깨질 때만)

## 볼 위치
- `backfill.rs`의 `link_stored_headlines`(이 함수는 3일치 `feed_headlines`를 보므로 쓰지 않는다)
- `collect/run.rs`의 `load_match_stocks`, 연결 저장(`article_matches` INSERT, 같은 기사–항목 중복 처리)
- `evaluate/run.rs`의 `EVALUATION_WINDOW_MS`
- `worker/index.rs`의 `start`(`close_stale_runs` 다음)
- `settings.rs`의 `TOPICS_REFRESHED_AT`(시각을 담는 숫자 설정 모양)

## 미리 정한 판단
- 대상: `articles.published_at >= now - EVALUATION_WINDOW_MS`인 기사, 지우지 않은 `kind = 'market'` 항목. `create_matcher`에 시장 항목만 넣고 `find(title, summary 또는 "")`로 맞춘다. 이미 같은 기사–항목 연결이 있으면 건너뛴다.
  이유: 결정 ⑮(최근 7일만), 계획 답 ⑩. 7일 창은 평가 대기 창과 같아 걸린 것은 모두 평가된다.
- 표시는 숫자 설정 `classify.marketRelinkedAt`(기본 0). 0이 아니면 하지 않고, 끝나면 그 시각을 쓴다. 기사가 0건이어도 쓴다.
  이유: 계획 답 ⑩(worker 시작 때 한 번, 설정 값 표시). 버린 대안: 배포 때 수동 명령
- 로그 한 줄: `시장 항목 다시 걸기: 기사 N건 확인, 새 연결 M개`(지금 로그 문체를 따름).
- 성격 7일 묻기(⑨)는 이 작업에 넣지 않는다. `article-nature`의 묻기 대상이 7일 창 안의 성격 없는 보이는 기사라 저절로 한 번 된다.

## 체크포인트 (각각 검사 통과 후 커밋)
1. 다시 걸기 함수와 표시 → `backfill_tests` 새 시험 통과
2. worker 시작에서 부르기 → 시험 통과

## 제약
- 기사·종목 연결·평가 줄은 지우거나 고치지 않는다(새 연결 INSERT만).
- 다시 걸기 중에 AI를 부르지 않는다.

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고)

## 알려진 함정
- 리뷰(2026-11-14, market-alias): `pnpm format:check`는 `work/article-classify/measure/first-response.json` 때문에 이미 실패한다(이번 작업과 무관, 고치지 말고 인계에 적는다). 시장 항목 `find`는 별칭이 맞으면 이름 연결(`match_kind = 'name'`, 경로 항목 이름, level 0)을 돌려주며, 이미 키워드로 걸린 기사–항목은 건너뛴다.
