# nature-tabs 인계
커밋: 0cd5a98 | 상태: 완료

> 넣지 않는 것: 코드 설명(커밋이 기억함), 작업 경위, 고생한 과정.

## 검사 결과 (실행한 명령과 출력 숫자)
| 명령 | 출력 숫자 |
|---|---|
| `cargo fmt --check` | 차이 0 |
| `cargo clippy --all-targets -- -D warnings` | 경고 0 |
| `cargo test` | 776 통과, 0 실패 (새 시험: read_tests 5개, timeline_tests 3개) |
| `pnpm format:check` / `lint` / `typecheck` | 모두 통과 |
| `pnpm test` | 7 통과 |
| `./build.sh --release` | 성공 |
| `pnpm test:e2e` | 53 통과, 5 건너뜀(설계상), 0 실패. 새 시험: timeline 성격 탭(데스크톱·모바일), guest 성격 탭(데스크톱·모바일) |
| 캡처(직접 확인) | `work/article-classify/nature-tabs/desktop-fact.png`, `desktop-view.png`(1280px), `mobile-fact.png`, `mobile-view.png`(모바일 프로젝트 화면, 390px 기기 배율 적용). 탭 한 줄 같은 너비·지금 탭 아래 강조·제목 앞 회색 태그 확인 |

## 브리프에 없던 결정
- E2E는 가짜 TypeSafe를 거치지 않고 DB에 성격 값을 직접 넣었다. 가짜 RSS에 "[포토]"·"전망" 제목이 없고 `mock-services.mjs`가 수정 범위 밖이며, 타임라인 시험은 DB 직접 삽입 방식이라서다. 값은 `natureAnswer` 규칙과 같다(포토 media 0.9, 전망 forecast 0.9·fact 0.2, 그 밖 fact 0.9). 삽입 함수는 `e2e/helpers.ts`의 `seedNatureFixtures`(timeline·guest 공용).
- 기존 시험 `read_tests.rs`의 `set_nature` 도우미가 `nature_fact = 1.0`도 넣도록 바꿨다(기대값은 그대로). 이유: 사실 탭은 `nature_fact >= ?`라 fact가 NULL이고 `nature_at`만 있는 기사는 안 보인다.
- `TimelineFilter`에 필드가 생겨 기존 시험의 구조체 직접 값 3곳(`ALL`, `parse_timeline_params` 시험, `timeline_href` 시험)에 `nature: NatureTab::Fact`를 더했다(기대값 그대로).
- 주소 순서: `stock`·`topic`·`impact`·`nature=view`·`limit`. 사실 탭은 값이 없다.
- 태그는 카드에 묶인 기사 모두의 합이고 탭에 상관없이 그 기사의 전체 태그를 보인다(사실+전망 기사는 두 탭 모두 "사실 전망"). 탭에 걸리는 후보만 묶는다.
- 탭 SQL은 NULL 확률에 COALESCE를 쓰지 않고 브리프 글자 그대로다. 성격이 저장될 때는 여섯 값이 함께 저장된다는 전제.
- askama가 `&`를 `&#38;`로 내보내 시험의 href 비교에 그 글자를 썼다.

## 발견한 것 (함정, 실패 교훈, 기존 코드의 버그)
- 없음.

## 상위로 올린 것
- 없음.

## 다음 작업에 영향
- 명세 FR-7에 넣을 것: 타임라인에 "사실"·"해석·전망·기타" 두 탭(주소 `nature=view`, 기본 사실, 주가 띠·시장 이슈는 탭과 무관). 사실 탭은 사실 태그 기사와 성격이 아직 없는 기사, 다른 탭은 해석·전망·기타 태그 기사(태그 여럿이면 각 탭에). 카드 제목 앞에 성격 태그 칩(사실·해석·전망·기타, 회색). 성격 없는 기사는 태그 없음. 같은 소식 묶기는 그 탭 조건을 넘은 기사끼리, 카드 태그는 묶인 기사 태그의 합.
- 범위 밖 변경 없음.
