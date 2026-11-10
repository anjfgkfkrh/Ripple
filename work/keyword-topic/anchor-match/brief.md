# anchor-match: 별칭으로 걸기, 고유어 없는 종목 키워드 연결 거르기
모델: Sonnet / effort: medium | 선행: anchor-schema 통과

## 목표
종목의 별칭을 종목 이름처럼 걸고, 종목의 키워드 연결(구조 키워드·토픽 가지)은 제목에 그 종목의 고유어가 없으면 `gated = 1`로 저장해 평가에서 뺀다. 수집과 보관 제목으로 지난 기사 걸기 모두에 적용한다.

## 완료 조건
- `match_tests` 새 시험 통과: 별칭이 제목·요약에 있으면 이름 연결(0단계, 경로는 종목 이름), 키워드만 맞고 제목에 고유어가 없으면 `gated`, 요약에만 고유어가 있으면 `gated`, 제목에 고유어가 있으면 걸러지지 않음, 고유어 목록이 비면 `gated`, 시장·주제 항목은 고유어와 상관없이 걸러지지 않음, 이름 연결이 키워드 연결보다 앞섬(지금과 같음)
- `collect/run_tests` 새 시험 통과: 걸러진 연결이 `article_matches.gated = 1`로 저장됨
- `backfill_tests` 새 시험 통과: 지난 기사 걸기도 같은 규칙
- `evaluate/run_tests` 새 시험 통과: `gated = 1` 연결은 `pending_evaluations`·`count_pending_evaluations`에 나오지 않음
- `cd rust && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` 통과

## 수정 가능 범위
- `rust/ripple/src/server/collect/match.rs`
- `rust/ripple/src/server/collect/match_tests.rs`
- `rust/ripple/src/server/collect/run.rs`
- `rust/ripple/src/server/collect/run_tests.rs`
- `rust/ripple/src/server/collect/backfill.rs`
- `rust/ripple/src/server/collect/backfill_tests.rs`
- `rust/ripple/src/server/evaluate/`

## 볼 위치
- `server/collect/match.rs`의 `create_matcher`, `Matcher::find`, `contains_word`
- `server/collect/run.rs`의 `run_source`, `merge_matches`, `save_items`, `load_match_stocks`
- `server/collect/backfill.rs`의 `link_stored_headlines`
- `server/evaluate/run.rs`의 `pending_evaluations`, `count_pending_evaluations`
- `docs/DECISIONS.md` 1장 "17단계 계획 때 정한 세부" ⑤⑥, "17단계 `keyword-topic` 계획 답" ⑦

## 미리 정한 판단
- `Matcher::find`는 제목과 요약을 따로 받는다(`find(title, summary)`). 이름·별칭·키워드 적중은 지금처럼 "제목 + 공백 + 요약"에서 찾고, 고유어는 제목에서만 찾는다.
  이유: 결정이 "제목에 고유어"다. 버린 대안: 합친 글에서 찾기
- 고유어 찾기: 제목과 고유어를 모두 소문자로 바꾸고 공백을 뺀 뒤 단순 포함(substring)으로 본다. 영문 낱말 경계는 보지 않는다.
  이유: 실측(`scripts/analysis/keyword-topic/anchors_llm.py`)과 같은 규칙이다. 'HBM'이 'HBM4'에 걸려야 한다. 버린 대안: `contains_word`(낱말 경계)
- 별칭은 이름과 같은 0단계 항(`MatchKind::Name`, 경로·이유는 이름과 같음)으로 넣는다. 이름 항 바로 뒤에 둔다.
- 걸러짐은 종목 종류(`ItemKind::Stock`)의 키워드 적중에만 한다. 이름·별칭·직접(Finnhub) 연결과 시장·주제 항목은 걸러지지 않는다. `StockMatch`에 `gated: bool`을 더하고 `save_items`가 `article_matches.gated`에 쓴다.
  이유: 사용자 답 7B(저장하되 평가하지 않음).
- 기사에 걸러진 연결만 있어도 기사를 저장한다(지금 "하나라도 걸린 기사만 저장" 규칙 그대로).
- `load_match_stocks`는 `stocks.aliases`·`anchors`를 읽어 `MatchStock`에 넣는다.

## 체크포인트 (각각 검사 통과 후 커밋)
1. `match.rs` 별칭·고유어 조건 → `match_tests` 새 시험 통과
2. 수집·지난 기사 걸기 저장 → `collect/run_tests`·`backfill_tests` 새 시험 통과
3. 평가 대기에서 빼기 → `evaluate/run_tests` 새 시험 통과

## 제약
- 기존 시험의 기대값은 바꾸지 않는다. 기존 시험 데이터의 종목은 고유어가 비어 키워드 연결이 걸러지므로, 키워드 연결을 평가하는 기존 시험이 깨지면 그 시험의 종목에 고유어를 넣어 맞춘다(기대값은 그대로).
- `docs/`는 고치지 않는다(인계에 적음).

## 알려진 함정
- 같은 종목에서 키워드 적중이 여럿이면 지금처럼 가장 가까운 것 하나만 고른다. 걸러짐은 그 하나에 붙는다.
- E2E(`e2e/collect.spec.ts`)의 시험 종목도 고유어가 비어 있다. `/check`를 돌리지 않는 작업이지만, 깨질 수 있는 곳을 인계에 적는다.

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고)
