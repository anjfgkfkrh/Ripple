# 키워드 트리 유용성 조사 (2026-11-16)

17단계 뒤 키워드 트리가 타임라인에 얼마나 기여하는지 잰 자료다. 결과 숫자는 `docs/DECISIONS.md` 2장 "키워드 트리 유용성 조사".

| 파일 | 내용 |
|---|---|
| `routes.py` | 경로별(이름·직접·구조 키워드·토픽 가지·시장 트리) 연결·`gated`·평가·통과, 키워드별 수, 키워드로만 통과한 기사 제목 |
| `counterfactual.py` | 시장 키워드 연결 중 18단계 고정 별칭과 겹치는 것, 트리 없이 고유어만으로 걸 때 늘어나는 기사, 창 안 비용 |
| `rates.py` | 경로별 통과율(복사 평가 제외), 18단계 대체 숨김 규칙, 평가 단가. 17단계 전 창 비교에도 쓴다 |
| `per_item.py` | 항목별 이름·키워드 연결과 통과 |

입력은 원격 본 DB 사본이다(받는 법은 `../article-classify/README.md` 1번). 2026-11-16에는 `.check-restore/ripple.db`(마지막 수집 11-14 04:00, `user_version` 12로 18단계 전)를 세션 임시 폴더에 복사해 읽었다. 끝나면 사본을 지운다(실제 데이터).

```sh
python3 -I routes.py $S/db/ripple.db "2026-11-11 16:03" > win.txt
python3 -I counterfactual.py $S/db/ripple.db "2026-11-11 16:03"
python3 -I rates.py $S/db/ripple.db 1794380580000 1794596400000 5   # 17단계 뒤, 영향도 기준 5
python3 -I rates.py $S/db/ripple.db 1794078000000 1794292200000 5   # 17단계 전 같은 길이
python3 -I per_item.py $S/db/ripple.db 1794380580000 1794596400000
```

18단계 뒤 사본에서는 시장 고정 별칭이 `match_kind = 'name'`으로 따로 걸리므로 `counterfactual.py` 1번 대신 `routes.py`의 `market:name` 줄을 본다. 사진영상·뉴스 모음 숨김(`nature_*`)은 이 스크립트들이 반영하지 않는다.

## 이슈 추적 점검 (2026-11-16)

결과는 `docs/DECISIONS.md` 2장 "키워드 트리 이슈 추적 점검".

| 파일 | 내용 |
|---|---|
| `page_trees.py` | 공개 종목 화면 HTML에서 지금 트리(경로·단계·4주 기사·통과)를 TSV로 뽑는다. 본 DB 없이 지금 트리를 볼 때 쓴다 |
| `issues.py` | 이슈 낱말(스크립트 안 `ISSUES`)이 제목에 있는 기사가 그 항목에 이름·키워드·`gated`로 걸렸는지, 통과했는지, 안 걸린 예 |

```sh
python3 -I issues.py $S/db/ripple.db 1794380580000
```

`issues.py`의 낱말은 부분 일치라 엉뚱한 기사(대우건설 노조 등)도 세므로, 안 걸린 예는 사람이 읽고 고른다.
