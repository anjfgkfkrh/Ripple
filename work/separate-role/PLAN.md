# separate-role 계획
목표: 손님은 비밀번호 없이 타임라인·종목을 보고, 상태·설정·수정은 비밀번호(주인 로그인 30일 + 수정 권한 30분) 뒤에 한다. 계정 기능으로 넓힐 수 있는 권한 구조로 만든다.

> 상태 값: 대기 / 진행 / 질문 / 되돌리기 / 통과 / 반려 / 보고 / 사람
> 시도: 작업을 worker에게 맡긴 횟수(숫자)

| 작업 | 목표 | 완료 조건 | 모델/effort | 선행 | 상태 | 시도 |
|---|---|---|---|---|---|---|
| access-token | 세션 payload에 `sub`·`role`·`edit_exp`, `Access` 값, `GUEST_VIEW`·`EDIT_UNLOCK_MINUTES` 설정 | `session_tests`·`config_tests`의 새 시험 통과, `/check` 전체 통과 | Sonnet / medium | 없음 | 대기 | 0 |
| role-guard | guard 기본 닫힘 분류, `POST /unlock`, `noindex`, 상단·배너 손님 처리, 기존 E2E 맞추기 | `guard_tests`·`login_tests`의 새 시험, 고친 `auth.spec.ts` 통과, `/check` 전체 통과 | Sonnet / medium | access-token 통과 | 대기 | 0 |
| guest-limits | DB 읽기 `spawn_blocking`, 손님 GET IP별 1분 60번, 손님 타임라인 최대 90개, 손님에게 키워드 실패 이유 숨김 | `guest_limit` 관련 새 단위 시험과 `read_tests` 새 시험 통과, `/check` 전체 통과 | Sonnet / medium | role-guard 통과 | 대기 | 0 |
| unlock-ui | 비밀번호 창과 원래 요청 다시 보내기, 손님 읽음 `localStorage`, 손님 E2E, README·INSTALL·`.env.example`(Cloudflare 요청 제한 안내 포함) | 새 `e2e/guest.spec.ts` 통과, `/check` 전체 통과, 손님·비밀번호 창 화면 캡처 | Sonnet / medium | guest-limits 통과 | 대기 | 0 |

## 사람이 승인한 큰 결정
- DECISIONS.md 1장 "16단계: 보기·수정 권한 나누기" 줄(사용자 답 "1A 2B 3C 4A 5A, 6: 1A 2서버설정파일 3A 4B 5A")
- 계획 세부(Access 구조, 기본 닫힘 분류, 응답 모양, `/unlock`, 값 범위, 메뉴·배너, 손님 읽음 저장)는 같은 장 "16단계 계획 세부" 줄
- 공개 보안 대책(사용자 답 "1A 2A 3A 4B 5A")과 그 세부는 같은 장 "16단계 공개 보안 대책"·"16단계 보안 대책 계획 세부" 줄
