# 키워드 트리·토픽 고도화 조사 스크립트 (2026-11-10)

결과는 `docs/DECISIONS.md` 2장 "키워드·토픽 고도화 조사"·"키워드·토픽 고도화 유료 확인", 결정은 1장 "키워드 트리·토픽 고도화" 줄들에 있다. 일회성 스크립트라 테스트는 없다. python3와 표준 라이브러리만 쓴다.

## 준비

본 DB는 건드리지 않는다. 원격 백업을 작업 폴더(저장소 밖 임시 폴더)에 `db/ripple.db`로 받아 읽기 전용으로 연다. 끝나면 사본을 지운다(실제 데이터다).

```bash
W=<임시 폴더>; mkdir -p $W/db $W/paid/req/jev $W/paid/req/or $W/paid/res/jev $W/paid/res/or
F="$(ssh blockall@blockall-ss 'ls -t ~/Ripple/backups/*.db | head -1')"
scp -q "blockall@blockall-ss:$F" $W/db/ripple.db
```

## 무료 분석 (`python3 <스크립트> $W/db/ripple.db`)

| 파일 | 재는 것 |
|---|---|
| `funnel.py` | 연결 방식별 호출·통과, 제목/요약 적중, 키워드별 수율, "통과 0이면 중단" 시뮬레이션 |
| `duplicates.py` | 자동 토픽 한국·미국 쌍, 같은 기사의 영향도 일관성, 트리 점수와 수율, 날마다 트리 겹침 |
| `topic_input.py` | 토픽 갱신 입력 줄이기(일본어·같은 소식 묶기), 피드별 적중, 별칭만 있는 제목 |
| `anchors_hand.py <db> <출력 폴더>` | 손으로 정한 앵커 규칙, 토픽별 수율, 눈가림 표본(`blind_titles.txt`·`blind_key.json`) |
| `stat_words.py` | 이름으로 걸려 통과한 기사 제목에 유난히 자주 나오는 낱말 |
| `exclusion.py` | 기사 수가 적은 키워드의 통과 몫, 주 단위 제외 규칙이 놓치는 통과 |

## 유료 확인 (비용 승인 뒤에만)

요청 파일을 만들고, 시험용 컨테이너 안에서 `run.sh`가 curl로 보낸다. 키는 컨테이너 환경 변수에서만 읽는다(compose가 본 env 파일을 읽어 넣어 준다). 시험 DB와 앱 흐름은 쓰지 않는다.

```bash
cd $W
python3 <저장소>/scripts/analysis/keyword-topic/jev_make.py db/ripple.db paid        # Jev 요청 345개
python3 <저장소>/scripts/analysis/keyword-topic/llm_make.py db/ripple.db paid first  # 트리·토픽 요청
cp <저장소>/scripts/analysis/keyword-topic/run.sh paid/
cd <저장소>   # Docker가 켜져 있고 ripple-app 이미지가 있어야 한다
TUNNEL_TOKEN= PUBLIC_URL= WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups KIS_APP_KEY= KIS_APP_SECRET= FINNHUB_API_KEY= \
  docker compose -p ripple-check run --rm --no-deps -v "$W/paid":/work --entrypoint sh worker /work/run.sh jev   # 또는 or
```

- `llm_make.py … second`는 첫 결과(`m2_v1_*_r1`)를 지금 트리로 삼아 주간 점검 요청을, `third`는 넓힌 고유어 문구(앞 7일 낱말만 참고) 요청을 만든다. 만든 뒤 `run.sh or`를 다시 돌린다(응답이 있는 요청은 건너뛴다).
- 읽기: `$W`에서 `jev_read.py`, `llm_read.py`, `anchors_llm.py`, `anchors_llm_split.py`(인자 없음, `db/ripple.db`와 `paid/`를 상대 경로로 읽는다).
- 끝나면 `docker compose -p ripple-check down -v`(앞머리 변수 포함)와 `rm -rf .check-backups $W/db`.
- 2026-11-10 비용: Jev 345회 $0.0164, OpenRouter 37회 $0.0670.

## 이 스크립트에 든 프롬프트 초안

구현 단계에서 출발점으로 쓴다(실측한 문구).

- `llm_make.py` `new_full_messages`: 새 전체 생성(깊이 2, 1단계 6·2단계 12 상한, 별칭·고유어). `BROAD = True`일 때의 고유어 문구가 실측에서 손 앵커와 같은 수준이었다.
- `llm_make.py` `update_messages`: 주간 점검(뺄 것·더할 것만).
- `llm_make.py` `topic_system("B")`·`reduce_titles`: 줄인 입력과 `scope` 칸.
- `jev_make.py` `merged_request`와 (c) 부분: 한국·미국 합친 질문, 기사당 한 요청의 state·questions 모양.
