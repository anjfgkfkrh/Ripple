# 기사 분류 조사 (2026-11-13)

종목 이름으로 걸려 화면에 보이는 기사 중 지수·다른 대상이 주어인 오분류를 찾은 자료다. 결정은 `docs/DECISIONS.md` 1장 "기사 분류 개선", 결과 숫자는 2장 "기사 분류 조사".

| 파일 | 내용 |
|---|---|
| `export_shown.py` | DB 사본에서 이름 연결 중 화면에 보이는 것을 TSV로 뽑는다 |
| `classify.py` | 제목 규칙으로 ①(지수 기사, 이름은 요약에만) ②(뉴스 모음) ④(지수 기사, 제목에 이름)로 나눈다. 입력 `remote_shown.tsv` |
| `labeled-2026-11-13.txt` | 원격 DB 사본(2026-11-13 17:08)에 위 규칙을 돌린 뒤 사람이 고친 결과(규칙 오판을 옮기고 빠진 것을 더함). 다음 단계의 일관성 측정 표본으로 쓸 수 있다 |

## 다시 돌리기

1. 원격 본 DB 사본을 받는다(본 Docker는 읽기만). `-wal`을 빼면 최근 기사가 빠지고, `docker compose cp`에는 `-q`가 없다.
   ```sh
   S=<Mac 세션 임시 폴더>; mkdir -p $S/remote
   ssh blockall@blockall-ss 'D=$(mktemp -d) && cd ~/Ripple && docker compose cp worker:/data/ripple.db $D/ripple.db >/dev/null && docker compose cp worker:/data/ripple.db-wal $D/ripple.db-wal >/dev/null && tar -C $D -cz . && rm -rf $D' | tar -C $S/remote -xz
   ```
2. `python3 -I export_shown.py $S/remote/ripple.db > remote_shown.tsv`. 2026-11-13에 쓴 TSV는 1,463줄이었는데 이 스크립트는 같은 사본에서 1,531줄을 낸다(그때 쿼리와 조건이 조금 다르다. 차이는 확인하지 않음).
3. 같은 폴더에서 `python3 -I classify.py > classified.txt`. 규칙은 거칠어서 사람이 제목을 읽고 고쳐야 한다(2026-11-13에는 정상 기사 5건을 빼고, 7건을 ④로, 2건을 ①로 옮기고, 1건을 더했다).

③(다른 회사·기관이 받는 영향)은 규칙으로 못 잡아 공개 화면을 보고 사람이 골랐다(`labeled` 파일에는 없음, 2장 숫자 참고).

## measure 단계 측정 (2026-11-14)

결과는 `work/article-classify/measure/handoff.md`(상위가 `docs/DECISIONS.md` 2장에 옮김). 입력은 원격 DB 사본(`.check-restore/ripple.db`)을 세션 임시 폴더에 복사한 것이다. 끝나면 사본을 지운다(실제 데이터).

| 파일 | 내용 |
|---|---|
| `measure_free.py` | 무료 수치 a~d(7일 건수, 성격 묻기 비용, 지난 평가 규칙으로 숨는 수, 시장 별칭 걸림) |
| `measure_make.py` | 표본 50건(시드 고정)과 성격·종목 요청 300개 만들기 |
| `run.sh` | 컨테이너 안에서 curl로 보냄(`keyword-topic/run.sh`와 같음) |
| `measure_read.py` | 응답을 읽어 일관성 표 |

다시 돌리는 순서(`S`는 세션 임시 폴더, 저장소 루트에서):

```sh
mkdir -p $S/db $S/paid && cp .check-restore/ripple.db $S/db/ripple.db
python3 -I scripts/analysis/article-classify/measure_free.py $S/db/ripple.db > free.txt        # 무료
python3 -I scripts/analysis/article-classify/measure_make.py $S/db/ripple.db $S/paid first      # 첫 요청 1개만
cp scripts/analysis/article-classify/run.sh $S/paid/
export W=$S/paid
TUNNEL_TOKEN= PUBLIC_URL= WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups KIS_APP_KEY= KIS_APP_SECRET= FINNHUB_API_KEY= \
  docker compose -p ripple-check run --rm --no-deps -v "$W":/work --entrypoint sh worker /work/run.sh jev   # 유료(비용 승인 뒤)
python3 -I scripts/analysis/article-classify/measure_make.py $S/db/ripple.db $S/paid               # 300개 (응답 있는 것은 건너뜀)
# 위 docker 명령을 다시 실행
python3 -I scripts/analysis/article-classify/measure_read.py $S/paid > consistency.txt
TUNNEL_TOKEN= PUBLIC_URL= WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups KIS_APP_KEY= KIS_APP_SECRET= FINNHUB_API_KEY= docker compose -p ripple-check down -v
rm -rf $S/db $S/paid; rmdir .check-backups
```

- Jev 예/아니오 질문의 `type`은 `"boolean"`이 아니라 `"noul"`이다(SDK 0.7.0 `dist/index.d.mts`). `"boolean"`은 HTTP 400 `Invalid request.`. 응답은 `answers.<이름> = {"type": "noul", "noul": <예일 확률 0~1>}`.
- 요청·응답에는 기사 제목·요약이 들어 있어 커밋하지 않는다(`work/.../measure/data/`는 `.gitignore`).
