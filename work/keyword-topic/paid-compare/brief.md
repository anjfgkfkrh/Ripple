# paid-compare: 추론 토큰 끄기·토픽 통과 건수 유료 비교
모델: Sonnet / effort: medium | 선행: 없음

## 목표
승인된 유료 비교 두 가지를 돌려 수치만 모은다. 쓸지 말지는 정하지 않는다(리뷰가 정함).
(a) OpenRouter에서 숨은 추론 토큰을 끄거나 줄이는 요청 값과, 끈 것·지금의 출력·비용·품질 차이
(b) 토픽 갱신의 이전 토픽 목록에 "· 통과 M건"을 보일 때와 안 보일 때의 토픽 유지·끝냄 차이

## 완료 조건
- `handoff.md`에 다음을 적는다
  - OpenRouter 공식 문서에서 찾은 추론 설정 값(문서 주소와 인용 한두 줄), 그 값이 `provider.require_parameters: true`와 함께 쓰일 수 있는지(실제 응답으로 확인)
  - (a) 표: 요청마다 입력·출력·추론 토큰(`completion_tokens_details.reasoning_tokens`), 비용(`usage.cost`), 형식 통과 여부, 트리는 단계별 개수·상한 준수·별칭·고유어 개수, 토픽은 토픽 수·같은 id 수
  - (b) 표: 창마다 두 방식의 토픽 수, 이어짐·새로·뺀 토픽, 통과 0건 토픽 가운데 뺀 수와 남긴 수, 통과가 있던 토픽 가운데 뺀 수
  - 실제 쓴 비용 합계(OpenRouter 응답의 `usage.cost` 합)
- 합계가 $0.08을 넘지 않는다(넘을 것 같으면 그 전에 멈추고 보고)
- 시험 compose를 내리고(`down -v`) `.check-backups`와 작업 중 만든 DB 사본을 지웠다

## 수정 가능 범위
- `work/keyword-topic/paid-compare/`

## 볼 위치
- `scripts/analysis/keyword-topic/README.md` 전체(준비·유료 확인·정리 절차)
- `scripts/analysis/keyword-topic/llm_make.py`: `new_full_messages`(`BROAD = True`일 때), `topic_system("B")`, `reduce_titles`, `topic_user`, `put`
- `scripts/analysis/keyword-topic/run.sh`, `llm_read.py`
- `rust/ripple/src/server/ai/llm/openrouter.rs`의 요청 본문(`provider.require_parameters`)
- `docs/DECISIONS.md` 2장 "키워드·토픽 고도화 유료 확인"의 "추론 토큰" 줄

## 미리 정한 판단
- 입력 DB는 사용자가 받아 둔 `.check-restore/ripple.db`의 사본만 쓴다. 원격에 접속하지 않는다.
  이유: 원격 조회는 권한에서 막히고, 본 데이터는 건드리지 않는다. 버린 대안: 직접 `scp`
- 스크립트는 `scripts/analysis/keyword-topic/`를 이 작업 폴더로 복사해 고쳐 쓴다. 저장소의 원본은 고치지 않는다.
  이유: 일회성 비교이고 원본은 지난 실측의 기록이다.
- (a) 요청: 새 전체 생성 형식(`new_full_messages`, `BROAD = True`)을 종목·시장 항목마다 추론 끔 1번·지금 1번, 토픽 갱신 줄인 입력(`topic_system("B")` + `reduce_titles`)을 창 2개에 끔 1번·지금 1번. 모델은 `anthropic/claude-haiku-5.5`.
  이유: 지난 비교와 같은 형식이라 수치를 맞대 볼 수 있다. 버린 대안: 지금 앱 프롬프트로 비교(곧 바뀜)
- (b) 요청: 토픽 갱신 줄인 입력을 창 3개(DB의 최근 갱신 시각 셋)에 "통과 M건 보임"·"안 보임" 각 1번. 통과 건수는 화면 표시 기준(설정 `evaluation.minRelevance`·`evaluation.minMagnitude`, 영향도 없는 평가는 관련도만)으로 센 최근 24시간 수. 줄 모양은 `- {id}: {이름} — {설명} [{검색어}] · 기사 {N}건 · 통과 {M}건{ · 끝남}`. (a)에서 끄는 설정이 형식을 지키면 (b)는 끈 상태로 보낸다.
  이유: 결정 ④(DECISIONS 1장 "유료 확인 뒤 결정")의 비교 그대로다.
- 사용량은 본 DB `usage_logs`에 옮기지 않는다.
  이유: 사용자 답 6c-A(DECISIONS 1장 "17단계 `keyword-topic` 계획 답" ⑥).

## 체크포인트 (각각 검사 통과 후 커밋)
1. 문서 확인과 요청 파일 만들기 → 요청 수와 예상 비용(입력 토큰 × 단가)을 인계에 적음
2. 보내기·읽기 → 표 두 개와 비용 합계를 인계에 적음, 시험 compose 내림·사본 삭제

## 제약
- 본 Docker(원격 `blockall-ss`)와 Mac의 `-p ripple-check` 아닌 compose는 건드리지 않는다. compose 명령에는 README의 앞머리 변수(`TUNNEL_TOKEN= PUBLIC_URL= …`)를 붙인다.
- 키는 컨테이너 환경 변수에서만 읽고 출력하지 않는다. `.env`는 읽지 않는다.
- `docs/`는 고치지 않는다. 결론을 내리지 말고 수치만 적는다.

## 알려진 함정
- 추론이 붙는지는 같은 입력에서도 갈린다(지난 비교). 한 번 결과로 단정하지 말고 그대로 적는다.
- `.check-restore/ripple.db`가 없거나 읽기가 권한에서 막히면 그 자리에서 멈추고 보고한다.
- Docker가 꺼져 있거나 `ripple-app` 이미지가 없으면 보고한다(이미지를 새로 빌드하지 않는다).
- 진단(2026-11-10, 시도 1, sonnet/medium): 외부 문제 — Docker 데몬이 꺼져 있어(`docker.sock` 없음, 진단 때 다시 확인) 시험 compose를 띄우지 못함. 비용 $0, 커밋 없음, `.check-restore/ripple.db`는 있음. 사람이 Docker Desktop을 켜고 `ripple-app` 이미지가 있는지 확인한 뒤 같은 브리프·같은 단계로 재시도 → 사람에게

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고) + 비용이 $0.08을 넘을 것 같을 때, 문서에서 추론을 끄는 값을 찾지 못했을 때
