# measure: 무료 수치와 Jev 예/아니오 질문 유료 측정
모델: Sonnet / effort: medium | 선행: 없음

## 목표
DB 사본으로 이 단계 결정에 필요한 수치를 무료로 센다. 그다음 Jev 예/아니오(boolean) 응답 모양을 실제 호출 1번으로 확인하고, 표본 50건 × 3번으로 성격 질문 6개와 종목 질문 `bystander`의 일관성을 잰다. 좋고 나쁨은 판단하지 않고 수치만 모은다.

## 완료 조건
- `handoff.md`에 무료 수치 표(아래 "무료" a~d)
- `handoff.md`에 첫 유료 호출의 응답 원문(boolean 답 한 개의 JSON 모양 전체, 키 값은 없음)
- `handoff.md`에 일관성 표(아래 "유료" 보고)와 비용 합(응답의 입력 토큰 × $0.042/100만, $0.05 이하)
- 스크립트는 `scripts/analysis/article-classify/`에 있고 README에 다시 돌리는 순서가 있음
- 시험 compose를 내리고 DB 사본을 지웠다

## 수정 가능 범위
- `work/article-classify/measure/`
- `scripts/analysis/article-classify/`

## 볼 위치
- `scripts/analysis/article-classify/README.md`, `export_shown.py`, `labeled-2026-11-13.txt`(줄 모양: 분류 종목 종목관련도 시장관련도(0.0은 시장 평가 없음) S 날짜 제목)
- `scripts/analysis/keyword-topic/README.md`의 "유료 확인", `run.sh`(컨테이너 안 curl, 키는 환경 변수에서만)
- `rust/ripple/src/server/ai/decider/questions_fixture.json`의 `articleQuestions.stock`(지금 종목 질문 글자 그대로), `questions.rs`의 `build_article_state`(state 모양)
- `rust/ripple/src/server/timeline/read.rs`의 `display_rule`, `rust/ripple/src/server/settings.rs`의 `EVALUATION_MIN_*`(설정 값이 DB에 있으면 그 값)
- `docs/DECISIONS.md` 1장 "기사 분류 개선" ⑧⑮⑱, "18단계 `article-classify` 계획 답" ①③⑤⑥, 2장 "기사 분류 조사"

## 미리 정한 판단
- 입력은 사용자가 받아 둔 `.check-restore/ripple.db`를 세션 임시 폴더에 복사한 사본이다. 원격에 접속하지 않는다. 없으면 멈추고 보고한다.
  이유: 본 데이터는 읽기만 하고 원격 조회는 권한에서 막힐 수 있다.
- **무료**(python3 표준 라이브러리, 사본 읽기만, "최근 7일"은 사본의 가장 늦은 `articles.published_at`부터 7일):
  - a. 최근 7일 하루 평균: 저장 기사 수, 평가가 하나라도 있는 기사 수, 화면 기준(관련도·영향도)을 넘는 평가가 하나라도 있는 기사 수, 그 기사들 중 같은 소식(24시간 안, `similar_title`과 같은 기준이 어려우면 제목 완전 일치로 대신하고 그렇다고 적음) 앞선 기사가 있는 수.
  - b. 최근 7일 화면 기준을 넘는 기사 수 합(성격을 한 번 물을 건수). 비용 추정 = 건수 × 700토큰 × $0.042/100만.
  - c. 지난 평가 규칙: 종목 항목(`stocks.kind = 'stock'`) 평가 중 화면 기준을 넘는 것 가운데, 같은 기사의 시장 항목(`kind = 'market'`) 평가 관련도가 그 종목 관련도보다 큰 것. 종목별 수, 표본 ①·④ 제목과 같은 것 수(종목·제목으로 맞춤), 표본에 없는 것의 종목·두 관련도·제목 목록 전부(50건 넘으면 50건과 총수).
  - d. 별칭 구절 걸림(최근 7일 기사 제목+요약, 소문자·공백 하나로 맞춘 뒤 이어진 글자로 찾고, 영문·숫자 구절은 앞뒤가 영문·숫자가 아닐 때만): 한국 코스피·코스닥·KOSPI·KOSDAQ·국내 증시·국내증시·한국 증시, 미국 나스닥·S&P500·S&P 500·다우지수·다우존스·뉴욕증시·미국 증시·월가, 비교용 "다우". 별칭마다 기사 수, 증시마다 합(기사 단위)과 그중 그 증시 시장 항목 연결이 없는 수, "다우"만 걸리고 다우지수·다우존스는 아닌 기사 제목 20건, "월가"로만 걸린 기사 제목 20건. 비용 추정 = 연결 없는 수 × $0.000038.
- **유료**(최대 $0.05, 비용 합이 $0.04를 넘으면 그 자리에서 멈추고 보고):
  - 보내는 방법은 keyword-topic과 같다: `docker compose -p ripple-check run --rm --no-deps --entrypoint sh worker`로 컨테이너 안에서 curl을 돌린다(이미지가 없으면 `docker compose -p ripple-check build`). 앱 흐름과 시험 DB는 쓰지 않는다. 모델은 `jev-1.13.0`.
  - 첫 호출 1건: 아래 성격 요청 하나를 보내고 응답 원문을 저장한다. boolean 답에 0~1 확률을 담은 필드가 없으면 멈추고 보고한다.
  - 성격 요청: state `{"article": {"title", "summary"?}}`(요약이 비면 뺌), questions는 이 순서와 글자 그대로, 모두 `{"type": "boolean", "instructions": …}`:
    - `fact`: "Does the `article` report facts: events, announcements, results, or market moves that have already happened?"
    - `analysis`: "Does the `article` interpret the causes, meaning, or background of the news, beyond restating the facts?"
    - `forecast`: "Does the `article` present a prediction or outlook, such as a target price, an earnings estimate, or an expected market direction?"
    - `opinion`: "Is the `article` an editorial, column, or opinion piece arguing the writer's position?"
    - `media`: "Is the `article` mainly a photo or video, with only a caption or short description?"
    - `roundup`: "Is the `article` a roundup that lists several unrelated news items?"
  - 종목 요청: state는 `build_article_state`와 같은 모양(그 평가의 연결 경위·경로), questions는 `questions_fixture.json`의 `articleQuestions.stock` 셋 뒤에 `bystander`: `{"type": "boolean", "instructions": "Does the `article` mainly report how something else (a stock index, another company, or an institution) is affected, with the company in `stock` appearing only as a cause or in passing?"}`.
  - 표본 50건(기사–종목 평가 단위, 서로 다른 기사): 표본 ① 15, ④ 10, 표본에 없는 이름 연결 중 화면 기준을 넘고 제목에 그 종목 이름이 있는 회사 기사 15(무작위, 시드 고정), 제목 머리말로 고른 형식 기사 10([포토]·[사진]·[자막뉴스] 4, 뉴스 모음으로 보이는 것 3, [사설]·[기자수첩]·칼럼 3). 기사마다 성격 요청과 종목 요청을 3번씩(300회).
  - 보고: 질문마다 3번 모두 0.5 같은 쪽인 비율과 확률 평균·최소·최대를 묶음(①, ④, 회사 기사, 형식 기사)별로. `bystander`가 0.5 이상인 비율을 묶음별로. 성격 태그(사실·해석·전망, 셋 다 아니거나 `opinion`이면 기타, `media`·`roundup`은 숨김)의 묶음별 분포와 3번 모두 같은 태그 조합인 비율. 종목 요청의 관련도·영향도가 저장된 값과 다른 정도(평균 차이). 요청 종류별 평균 입력 토큰.
  이유: 결정 ⑱(일관성 측정 승인)·계획 답 ③⑪. 예/아니오 응답 모양은 문서에 원문이 없어 첫 호출로 확인한다(계획 세부 ⑦).
- 사용량은 본 DB에 옮기지 않는다. 요청·응답 파일은 이 작업 폴더의 `data/`에 두고, 기사 제목·요약이 들어 있으므로 커밋하지 않는다(`.gitignore`가 없으면 `data/`를 커밋에서 뺀다).

## 체크포인트 (각각 검사 통과 후 커밋)
1. 무료 스크립트와 a~d 수치 → 인계에 표
2. 첫 유료 호출과 응답 모양 → 인계에 원문
3. 표본 300회와 일관성 표, 내리기·사본 삭제 → 인계에 표와 비용

## 제약
- 본 Docker(원격)와 `-p ripple-check`가 아닌 compose는 건드리지 않는다. 시험 compose는 `-p ripple-check`만 쓴다.
- 키를 출력하지 않는다. `.env`는 읽지 않는다. `rust/`와 `docs/`는 고치지 않는다.
- 스크립트는 `python3 -I`로 돌린다.

## 알려진 함정
- `export_shown.py`는 같은 사본에서 2026-11-13 TSV(1,463줄)보다 많은 1,531줄을 낸다(README). 표본 맞추기는 종목·제목으로 한다.
- `docker compose cp`에는 `-q`가 없다. 컨테이너에 사본을 넣지 않아도 된다(요청 파일만 마운트).

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고) + boolean 응답에 확률 필드가 없음, 비용 $0.04 초과, `.check-restore/ripple.db` 없음
