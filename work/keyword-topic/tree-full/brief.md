# tree-full: 새 전체 생성(깊이 2·상한·별칭·고유어·통계 낱말), Jev 후보 평가 없애기
모델: Sonnet / effort: medium | 선행: llm-request 통과, anchor-match 통과

## 목표
트리를 새로 만들 때(새 항목, 0011 뒤 대기 중인 종목, "키워드 다시 만들기") LLM 한 번으로 깊이 2 트리를 상한 안에서 만들고, 종목은 별칭·고유어도 함께 받아 저장한다. 참고 자료는 "최근 주요 기사 제목 20개" 대신 통계 낱말이다. Jev 후보 평가와 점수로 고르기는 없앤다.

## 완료 조건
- `tree_tests` 새 시험 통과: 종목·시장·주제·미국 종목의 새 프롬프트가 `prompts/keywords.json`과 같음, 1단계 6개·2단계 12개를 넘는 후보는 버림, 깊이 1이면 1단계만, 응답 파싱(별칭·고유어·키워드)
- 통계 낱말 시험 통과: 낱말 쪼개기(조사 떼기, 2자 미만·숫자 버림), 문턱(3번 또는 3%), 점수 순서, 기사가 없으면 빈 목록
- `generate_tests` 새 시험 통과: Jev를 부르지 않음, 키워드·검증한 별칭·다듬은 고유어를 한 트랜잭션으로 저장, 종목인데 고유어가 0개면 실패하고 이전 트리·고유어를 그대로 둠, 시장 항목은 별칭·고유어를 저장하지 않음, 토픽 가지는 남김(지금과 같음)
- `/check` 전체 통과(E2E 포함)

## 수정 가능 범위
- `rust/ripple/src/server/keywords/`
- `rust/ripple/src/server/keywords.rs`
- `rust/ripple/src/server/prompts/keywords.json`
- `rust/ripple/src/server/ai/llm/schemas/keyword_tree.json`
- `rust/ripple/src/server/ai/llm/schemas.rs`
- `rust/ripple/src/server/ai/llm/schemas_tests.rs`
- `rust/ripple/src/worker/`(`GenerateOk` 칸이 바뀌어 깨지는 곳만)
- `rust/ripple/src/web/`(`GenerateOk`·키워드 칸이 바뀌어 깨지는 곳만)
- `e2e/mock-services.mjs`
- `e2e/keywords.spec.ts`(Jev 점수 "관련 7.0 · 영향 6.0" 확인만 뺌)

## 삭제할 파일
- `rust/ripple/src/server/keywords/headlines.rs`
- `rust/ripple/src/server/keywords/headlines_tests.rs`

## 볼 위치
- `scripts/analysis/keyword-topic/llm_make.py`: `target_words`, `new_full_messages`(`BROAD = True`일 때의 문구), `NEW_SCHEMA`, `KW_ITEM`, `toks`, `stat_words`
- `server/keywords/tree.rs` 전체, `server/keywords/generate.rs`의 `prepare_generation`·`try_generate`·`rate_candidates`·`replace_keywords`
- `server/stocks/aliases.rs`(anchor-schema의 `validate_aliases`), `server/stocks/repository.rs`의 `set_stock_terms`
- `server/evaluate/run.rs`의 `meets_display_rule`(통과 기준)
- `docs/DECISIONS.md` 1장 "키워드 트리·토픽 고도화 세부" ①④, "유료 확인 뒤 결정" 끝 문장(넓힌 고유어 문구), "17단계 계획 때 정한 세부" ①②⑤

## 미리 정한 판단
- 프롬프트 문구는 `llm_make.py`의 `new_full_messages`(`BROAD = True`)를 글자 그대로 옮긴다. 상한은 1단계 6·2단계 12. 깊이 설정이 1이면 깊이 줄을 "깊이는 1단계까지다. 1단계는 최대 6개를 넘기지 않는다. 넘기면 뒤의 것은 버려진다."로, 1·2단계 줄을 "- 1단계: {who}과 직접 이어지는 키워드. parent는 빈 문자열."로 바꾼다.
  이유: 실측 문구다(10번 모두 상한을 지킴, 고유어 수율이 손 앵커와 비슷). 버린 대안: 지금 프롬프트에 칸만 더하기
- 응답 형식은 `NEW_SCHEMA`(aliases·anchors·keywords)로 `keyword_tree.json`을 바꾸고 이름 `keyword_tree`는 그대로 둔다.
  이유: E2E 가짜 서버가 이 이름으로 답을 고른다.
- 깊이는 1~2로 자른다(`MAX_DEPTH = 2`). 코드에서도 단계마다 앞에서부터 6개·12개만 남긴다. 그 밖의 검증(부모·중복·대상 이름·시장 금지어)은 지금 그대로다.
- 참고 자료: 종목(kind=stock)만 사용자 메시지에 "통계 낱말" 줄을 넣는다. 계산은 `stat_words`를 옮긴 것이다.
  - 배경: `feed_headlines`의 모든 제목
  - 대상: 그 종목의 이름 연결(`match_kind = 'name'`) 평가 가운데 최근 14일에 평가했고 화면 표시 기준을 통과한 것의 서로 다른 제목
  - 문턱은 `max(3, 기사 수 × 0.03)`, 점수는 `(c/n) / ((배경 수 + 1)/배경 전체) × ln(c)`, 위에서 30개
  - 줄 모양은 `통과 기사에 자주 나온 낱말(참고 자료, 최근 14일 기사 {n}건): 낱말(수), …`. 낱말이 없으면 줄을 넣지 않는다.
  이유: 결정 세부 ①. 통과 기준은 화면 표시 기준으로 한다(세부 ②). 버린 대안: 최근 기사 제목 20개
- 저장: 별칭은 `validate_aliases`를 거친다. 고유어는 다듬어서(앞뒤 공백, 2자 미만·숫자만인 것·중복 버림) 최대 15개. 종목인데 고유어가 0개면 "고유어가 나오지 않았습니다." 오류로 실패하고 이전 것을 둔다. 키워드와 별칭·고유어는 한 트랜잭션으로 바꾼다. 시장·주제 항목은 별칭·고유어를 저장하지 않는다.
  이유: 고유어가 비면 종목 키워드 연결이 모두 걸러진다(세부 ⑥).
- Jev 후보 평가(`rate_candidates`), `prune_keyword_tree`, `KeepRule`, `MAX_KEPT_KEYWORDS`, `GenerateOptions.decider`의 키워드 쪽 쓰임은 없앤다. 새로 넣는 키워드의 `relevance`·`magnitude`는 NULL이다.
  이유: 결정 ③(Jev 후보 평가 없앰).
- 최대 출력 토큰은 8,000(지금 16,000).
  이유: 실측 최대 2,142토큰(추론 포함). 버린 대안: 그대로 16,000
- E2E 가짜 서버의 `keyword_tree` 답에는 별칭·고유어를 넣는다. 고유어는 기존 E2E 수집 표본 기사 제목에 실제로 있는 낱말로 골라, 기존 E2E의 키워드 연결 기대값이 그대로 맞게 한다.

## 체크포인트 (각각 검사 통과 후 커밋)
1. `tree.rs` 새 프롬프트·형식·상한, `prompts/keywords.json`, `keyword_tree.json` → `tree_tests`·`schemas_tests` 통과
2. 통계 낱말(새 파일) → 통계 낱말 시험 통과
3. `generate.rs` Jev 평가 없애기·저장, `headlines.rs` 삭제 → `generate_tests` 통과
4. E2E 가짜 서버·`keywords.spec.ts` → `/check` 전체 통과

## 제약
- 바꿔도 되는 기대값은 `prompts/keywords.json`, `tree_tests`·`generate_tests` 가운데 Jev 평가·점수로 고르기·최근 기사 제목·깊이 3~4를 다루는 시험(지우거나 새 규칙으로 고침), `e2e/keywords.spec.ts`의 점수 확인뿐이다. 그 밖의 기대값은 바꾸지 않는다.
- 실제 API를 부르지 않는다.
- `docs/`는 고치지 않는다(인계에 적음).

## 알려진 함정
- 실측에서 LLM이 별칭에 다른 회사 이름을 냈고, 깨진 값('삼전닉스제외')도 있었다. 검증을 반드시 거친다.
- 파이썬 `\w`는 유니코드 글자·숫자·밑줄이다. Rust 정규식 `\w`도 같다.
- `settings.rs`의 깊이 기본값·범위와 설정 화면은 tree-screen이 고친다. 여기서는 생성 쪽 자르기만 한다.
- 리뷰(2026-11-10, llm-request): `GenerateOk`에 `deepest`(후보 트리의 가장 깊은 단계)가 생겼고 결과 줄은 `queue.rs`의 `success_line`이 만든다(`rated`·`unrated_reason`을 쓴다). Jev 평가를 없애면 이 함수와 `queue_tests`의 `success_line_shows_actual_over_configured_depth`도 함께 고친다. 새 `StructuredRequest`에는 `reasoning: Reasoning::default()`를 넣는다.
- 리뷰(2026-11-10, anchor-schema): `validate_aliases`는 `rusqlite::Result<Vec<String>>`이고 `listings` 다른 줄 이름과 "서로 포함"을 공백 빼고 소문자로 단순 포함 비교한다. 짧은 상장사 이름(영문 미국 종목 등)이 있으면 별칭이 많이 버려질 수 있으니 시험 Docker 확인 때 버려진 수를 본다. `set_stock_terms`는 `&Connection`을 받으므로 트랜잭션(`Transaction`은 `Connection`으로 deref) 안에서 부른다.
- 리뷰(2026-11-10, anchor-match): 고유어 비교는 `match.rs`에서 제목과 고유어를 소문자로 바꾸고 공백을 모두 뺀 단순 포함이다(`squash`). 저장할 고유어를 다듬을 때 이 비교와 어긋나는 가공(낱말 경계 요구 등)을 하지 않는다. `gated`는 수집·지난 기사 걸기 때 정해져 저장되고, 고유어를 새로 만들어도 이미 저장된 연결은 다시 계산하지 않는다. E2E `collect.spec.ts`의 "하닉"(SK하이닉스) 키워드 연결도 고유어가 있어야 평가된다.

## 상위로 올릴 조건
기본 규칙(범위 밖·기대값 변경·두 번 고쳐도 남는 오류는 보고) + 기존 E2E 수집 기대값을 고유어로 맞출 수 없을 때
