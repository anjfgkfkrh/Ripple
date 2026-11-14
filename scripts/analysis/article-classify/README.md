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
