# stage-check: 시험 Docker 실제 AI 확인
모델: Sonnet / effort: medium | 선행: 앞 작업 8개 모두 통과

## 목표
본 백업 사본을 넣은 시험 Docker에서 실제 AI로 18단계 흐름을 한 번씩 돌리고 수치와 화면을 모은다. 흐름은 worker 시작 때 시장 다시 걸기, 수집·평가(새 종목 질문), 7일 성격 묻기다. 좋고 나쁨은 판단하지 않는다.

## 완료 조건
- `handoff.md`에 아래 수치와 비용(시험 DB `usage_logs`의 이 확인 중 늘어난 합, $0.15 이하)
  - 시장 다시 걸기 로그 줄(확인 기사 수, 새 연결 수), 증시별 새 연결 수
  - 평가: 회차마다 Jev 호출 수, 종목 평가 중 `bystander` ≥ 0.5 수와 그 제목 20건
  - 성격: 물은 수·복사한 수·실패 수, 태그별 기사 수, 숨김(사진영상·뉴스 모음) 수와 제목 20건
  - 표본 `scripts/analysis/article-classify/labeled-2026-11-13.txt` ①②④ 가운데 지금 그 종목에 보이는 수(분류별, 확인 전 백업과 나란히)
  - 7일 성격 묻기가 다 끝나지 않으면 남은 수와 회차 수
- 타임라인 두 탭(휴대폰·PC)과 종목 필터 하나의 캡처 경로
- 시험 compose를 내리고(`down -v`) `.check-backups`와 DB 사본을 지웠다

## 수정 가능 범위
- `work/article-classify/stage-check/`

## 볼 위치
- `.claude/skills/real-check/SKILL.md`(백업 사본 넣기, 실제 AI 확인 절차)
- `/Users/blockall/workspace/Ripple-archive/keyword-topic/stage-check/brief.md`·`handoff.md`(지난 단계 같은 확인)
- `docs/DECISIONS.md` 2장 "다시 잴 때의 방법"의 "worker 안 실제 AI 확인(10단계)" 줄(내부 API 호출, `keywords.autoRegeneratedAt`)
- `work/article-classify/measure/handoff.md`(무료 수치 b·d, 비교용)

## 미리 정한 판단
- 입력은 사용자가 받아 둔 `.check-restore/ripple.db`다. 원격에 접속하지 않는다. 없으면 멈추고 보고한다.
- 터널은 띄우지 않는다(`TUNNEL_TOKEN= PUBLIC_URL=`, `up -d migrate worker web`). AI 키는 env 파일 값을 그대로 쓰고 KIS·Finnhub 키는 비운다.
- 키워드 생성·토픽 갱신이 이 확인 중에 돌지 않게 한다. 띄우기 전 시험 DB에서 `keywords.autoRegeneratedAt`을 지금 시각으로, `topics.intervalHours`를 0으로 하고, 특정 시각 목록을 비운다. 값을 바꿨다고 인계에 적는다.
  이유: 이번 확인 대상이 아니고 OpenRouter 비용이 든다.
- 순서: 띄우기(다시 걸기 로그 확인) → 내부 API `POST /collect`를 성격 대기가 없어지거나 비용이 $0.12에 닿을 때까지 되풀이(회차 사이 대기는 회차가 끝날 때까지) → 수치·캡처.
  이유: 계획 답 ⑨⑩⑪
- 비용이 $0.15에 닿을 것 같으면 그 자리에서 멈추고 보고한다. 사용량은 본 DB에 옮기지 않는다.

## 체크포인트 (각각 검사 통과 후 커밋)
1. 띄우기·다시 걸기·수집 되풀이 → 수치 기록
2. 표본 다시 세기·캡처, 내리기·사본 삭제 → 수치·캡처 기록

## 제약
- 본 Docker(원격)와 `-p ripple-check`가 아닌 compose는 건드리지 않는다. 모든 compose 명령에 시험 앞머리 변수를 붙인다.
- 키를 출력하지 않는다. `.env`는 읽지 않는다. 코드와 `docs/`는 고치지 않는다.

## 알려진 함정
- 이미지는 이 브랜치로 새로 빌드해야 한다(`docker compose -p ripple-check build`, 몇 분). `ripple-app` 태그가 본 배포 이름과 같으니 시험 뒤 이미지를 지우지 않는다.
- 시장·주제 합친 평가는 사용량이 연결마다 한 줄이라 `usage_logs` 줄 수가 Jev 호출 수보다 많다. 호출 수는 worker 기록으로 센다.
- 백업 시각이 지금보다 오래되면 "최근 7일"은 지금 시각 기준이라 대상이 줄어든다. 백업의 가장 늦은 기사 시각과 지금 시각을 인계에 적는다.

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고) + 앱 오류(실패 상태·worker 로그 오류)를 만나면 고치지 말고 로그와 함께 보고
