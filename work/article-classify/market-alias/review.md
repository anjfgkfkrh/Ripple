# article-classify/market-alias 리뷰
결과: 통과
범위 밖 무기록: 0

## 범위 확인
- 검사 명령 재실행: 인계 숫자와 일치. `cargo test --lib` 737 passed, `cargo test 'r#match'` 31 passed, `cargo fmt --check` 통과, `cargo clippy --all-targets -D warnings` 경고 없음, `pnpm test:e2e` 49 passed·5 skipped. `pnpm format:check`는 `work/article-classify/measure/first-response.json` 하나로 실패(measure 작업이 커밋한 파일, 이번 변경과 무관).
- 범위 밖 판단: 없음(바뀐 파일은 `match.rs`·`match_tests.rs`·`run_tests.rs`와 작업 폴더·PLAN.md뿐. 미추적 `work/metrics.csv`·`work/article-classify/metrics.csv`는 작업 전부터 있음)

## Opus 확인
- 브리프에 없던 결정이 인계에 빠짐없이 있는가: 예. `Term.phrase`·`market_aliases`·`contains_phrase` 이름, 기존 시험 입력 변경, `seed_market` 사용.
- 테스트 기대값을 바꾸거나 빼서 통과시키지 않았는가: 예. `market_and_topic_items_match_by_keywords_only`의 입력 "한국 증시 마감 시황" → "증시 마감 시황"은 "한국 증시"가 별칭이 된 이번 기능의 직접 결과이고, "증시 하나로는 걸리지 않음"을 계속 확인한다. 주제·키워드 부분 기대값은 그대로.
- 제약과 프로젝트 규칙을 지켰는가: 예. 별칭 목록이 브리프와 같음, 사용자 별칭은 시장에 쓰지 않음(시험 `무시될 별칭`), `gate`는 종목만, 종목·주제 찾기 변경 없음. 구절 찾기가 정규화(소문자·공백 하나)한 글자에서 이어진 구절이고 영문·숫자 경계를 지킴("KOSPI200", "3월 가계" 시험).
- 인계가 실제 변경과 맞는가: 예.
- 참고: 제목과 요약을 이어 붙인 경계에서 구절이 걸칠 수 있으나(제목 끝 "한국" + 요약 앞 "증시") 드문 경우라 그대로 둔다.

## 반영한 것 (통과일 때)
- 영구 문서: DECISIONS 1장 새 줄 "18단계 market-alias 구현 세부"(①~③, 근거: article-classify/market-alias). SPEC 기사 걸기 장의 "시장·주제는 키워드로만" 줄을 시장 별칭 구절 규칙으로 고침. STATE 다시 씀(market-alias 통과, 다음 relink-7d, format:check 실패 원인 기록).
- 다음 브리프 확인: `relink-7d`는 이번 인계와 맞음(시장만 `create_matcher`에 넣고 `find`). 알려진 함정에 format:check 실패와 시장 `find` 결과 모양 한 줄을 더함.
