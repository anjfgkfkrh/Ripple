# stage-check: 시험 Docker 실제 AI 확인
모델: Sonnet / effort: medium | 선행: 앞 작업 9개 모두 통과

## 목표
본 백업 사본을 넣은 시험 Docker에서 실제 AI로 17단계 흐름(배포 직후 전체 생성, 정기 증분 점검, 수집·고유어 거르기·합친 평가, 토픽 갱신)을 한 번씩 돌린다. 수치와 화면을 모은다. 좋고 나쁨은 판단하지 않는다.

## 완료 조건
- `handoff.md`에 아래 수치와 비용(시험 DB `usage_logs` 합, $0.15 이하)을 적는다
  - 종목·시장 항목마다 전체 생성 결과(1단계·2단계 개수, 별칭·고유어, 검증에서 버린 별칭)
  - 정기 점검 결과(뺌·더함·지킴·밀어냄 수)
  - 수집 1회: 연결 방식별 새 연결 수와 그중 걸러진 수, Jev 호출 수와 평가 줄 수, 기준 통과 수
  - 시장 항목 통과 수를 백업의 같은 길이 직전 기간과 나란히(`scripts/analysis/keyword-topic/funnel.py` 방식)
  - 토픽 갱신 1회: 줄이기 전·뒤 제목 수, 토픽 수, company 토픽과 붙은 종목, 입력·출력·추론 토큰
- 종목 화면(트리 펼침·별칭)·타임라인·설정 화면 캡처 경로
- 시험 compose를 내리고(`down -v`) `.check-backups`와 DB 사본을 지웠다

## 수정 가능 범위
- `work/keyword-topic/stage-check/`

## 볼 위치
- `.claude/skills/real-check/SKILL.md`(백업 사본 넣기, 실제 AI 확인 절차)
- `docs/DECISIONS.md` 2장 "다시 잴 때의 방법"의 "worker 안 실제 AI 확인(10단계)" 줄(내부 API 호출, `keywords.autoRegeneratedAt` 다루기)
- `scripts/analysis/keyword-topic/funnel.py`, `README.md`
- `docs/DECISIONS.md` 1장 "17단계 `keyword-topic` 계획 답" ⑥⑧

## 미리 정한 판단
- 입력은 사용자가 받아 둔 `.check-restore/ripple.db`다. 원격에 접속하지 않는다.
  이유: 원격 조회는 권한에서 막히고, 본 데이터는 건드리지 않는다.
- 터널은 띄우지 않는다(`TUNNEL_TOKEN= PUBLIC_URL=`, `up -d migrate worker web`). AI 키는 env 파일 값 그대로, KIS·Finnhub 키는 비운다.
- 순서:
  1. 띄우면 0011이 종목을 대기 상태로 바꿔 worker가 전체 생성을 한다. 끝날 때까지 기다린다.
  2. 시장 항목 정기 점검: 시험 DB에서 그 항목의 `keywords_generated_at`을 지난주로, `keywords.autoRegeneratedAt`을 0으로 바꾼 뒤 06:00 계획이 돌게 한다(또는 내부 API).
  3. 내부 API `POST /collect` 1회.
  4. 상태 화면 "지금 갱신"과 같은 토픽 갱신 1회.
  이유: 결정 ⑧(배포 직후 1회 전체 생성)과 같은 흐름이다.
- 비용이 $0.15에 닿을 것 같으면 그 단계에서 멈추고 보고한다.
- 사용량은 본 DB에 옮기지 않는다.
  이유: 사용자 답 6c-A.

## 체크포인트 (각각 검사 통과 후 커밋)
1. 시험 Docker 띄우기와 전체 생성·정기 점검 → 수치 기록
2. 수집·토픽 갱신·캡처 → 수치·캡처 기록, 내리기·사본 삭제

## 제약
- 본 Docker(원격)와 `-p ripple-check`가 아닌 compose는 건드리지 않는다. 모든 compose 명령에 시험 앞머리 변수를 붙인다.
- 키를 출력하지 않는다. `.env`는 읽지 않는다. 코드와 `docs/`는 고치지 않는다.

## 알려진 함정
- 이미지는 이 브랜치로 새로 빌드해야 한다(`docker compose -p ripple-check build`, 몇 분). `ripple-app` 태그가 본 배포 이름과 같으니 시험 뒤 이미지를 지우지 않는다.
- `.check-restore/ripple.db`가 없거나 읽기가 막히면 멈추고 보고한다.

- 리뷰(2026-11-10, tree-weekly): 정기 점검은 트리와 고유어가 있어야 증분이다(없으면 전체 생성). 0011 뒤 첫 전체 생성이 끝난 다음 `keywords_generated_at`을 주기 시작 전으로 당기거나 `keywords.autoRegeneratedAt`을 지워 06:00 확인을 일으키면 점검 결과 줄 `키워드 점검: …`(시간·비용, 버린 별칭, 상한 밀어냄)을 잴 수 있다.
- 리뷰(2026-11-10, eval-merge): 시장·주제 항목은 기사당 Jev 한 요청이고 사용량은 연결마다 한 줄(토큰을 나눔)이라 `usage_logs` `article_evaluation` 줄 수는 Jev 호출 수보다 많다. 호출 수는 worker 기록으로 세고 비용은 줄 합으로 본다. 시장 항목 관련도(`relevance_kr`·`relevance_us`)는 실측에 없던 질문이다.
- 리뷰(2026-11-10, topic-input): 토픽 갱신 결과 줄의 제목 수(`summary.titles`)는 줄이기 전 수다. 줄인 뒤 수는 요청의 사용자 메시지 "새 뉴스 제목(최근순, N개)"의 N이라, 시험 DB의 `feed_headlines`로 `reduce_titles`와 같은 계산을 하거나 요청을 남겨 센다. company 토픽은 토픽 카드·토픽 항목이 없고 `topics.scope = 'company'`와 붙은 종목의 토픽 가지로 본다.

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고) + 앱 오류(실패 상태·worker 로그 오류)를 만나면 고치지 말고 로그와 함께 보고
