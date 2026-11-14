# article-classify/market-alias 인계
커밋: 6459463 | 상태: 완료

## 검사 결과 (실행한 명령과 출력 숫자)
| 명령 | 출력 숫자 |
|---|---|
| cargo test 'r#match' (match_tests) | 31 passed; 0 failed |
| cargo test alias_only_article (run_tests) | 1 passed; 0 failed |
| /check R1 cargo fmt --check | 통과 |
| /check R2 cargo clippy -D warnings | 경고 0 |
| /check R3 cargo test | 737 passed; 0 failed |
| /check R4 ./build.sh --release | 통과 |
| /check pnpm format:check | 실패, 경고 1 (`work/article-classify/measure/first-response.json`, 이번 작업 밖 파일) |
| /check pnpm lint / typecheck | 오류 0 / 오류 0 |
| /check pnpm test | 7 passed; 0 failed |
| /check pnpm test:e2e | 49 passed; 0 failed; 5 skipped([mobile]) |

## 브리프에 없던 결정
- 기존 시험 기대값 변경: match_tests.rs `market_and_topic_items_match_by_keywords_only`의 `find("한국 증시 마감 시황", "") == []`를 `find("증시 마감 시황", "") == []`로 바꿈 — "한국 증시"가 이제 별칭이라 원래 입력은 이름 연결이 걸림. 키워드로만 건다는 주제 부분과 키워드 부분은 그대로.
- 구현: `Term`에 `phrase: Option<String>`을 더하고 `market_aliases(Market)`·`contains_phrase`를 만듦(이름은 내가 정함). 별칭은 `ItemKind::Market`일 때만 붙이고 `normalize`한 구절로 비교.
- 수집 시험은 기존 `test_support::seed_market`을 써서 시장 항목을 만듦(test_support는 수정하지 않음).
- E2E는 그대로 통과해 mock-services·timeline.spec은 건드리지 않음.

## 발견한 것 (함정, 실패 교훈, 기존 코드의 버그)
- `pnpm format:check` 실패는 `work/article-classify/measure/first-response.json` 때문이며 이번 변경과 무관(고치지 않음).
## 상위로 올린 것
없음
## 다음 작업에 영향
시장 항목 이름 연결은 이제 별칭 구절로 걸리므로, 시장 항목 기사 수가 늘 수 있음.
