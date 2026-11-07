---
name: handoff
description: agentflow 단계 밖 작업(배포·급한 수정·릴리스·조사)을 마쳤거나 그런 세션을 끝낼 때 쓴다. 다음 세션이 이 대화 없이 이어 갈 수 있게 STATE.md와 DECISIONS.md를 고치고, 세션 이름을 제안한다.
---

다음 세션은 이 대화를 보지 못한다. CLAUDE.md, 메모리, `docs/STATE.md`만 보고 시작한다. agentflow 단계 작업은 쓰지 않는다(그 인계는 `work/<단계>/<작업>/` 파일과 review가 한다).

## 1. 모으기

이 세션에서 생긴 것: 받은 답·결정, 스스로 정한 작은 것, 한 일과 검사 결과(숫자), 실제 비용, 남은 일, 사용자에게 물어 두고 답을 못 받은 것.

## 2. 적기

| 무엇 | 어디에 |
|---|---|
| 사용자 결정 | DECISIONS.md 1장 표 끝(답과 함께, 예 "1A") |
| 스스로 정한 작은 것 | DECISIONS.md 1장 "— 구현 중 정함" |
| 실측·실제 확인 결과 | DECISIONS.md 2장 |
| 끝난 일의 자세한 기록(체크리스트, 검사 숫자, 배포) | DECISIONS.md 3장 맨 위 |
| 지금 하는 일, 답 대기, 다음 할 일 | STATE.md(짧게, 통째로 다시 씀) |
| 단계 상태·커밋 | DEVELOPMENT.md 2장 표 |
| 다시 쓸 절차·함정 | DEVELOPMENT.md 해당 장 또는 그 일을 하는 skill |
| 기능이 바뀐 것 | docs/SPEC.md 본문, README |

- STATE.md "마지막 갱신"은 `TZ=Asia/Seoul date '+%Y-%m-%d %H:%M'`로 적는다. 끝난 항목은 STATE.md에서 빼고 DECISIONS.md 3장으로 보낸다. STATE.md는 화면 한 장 안에 들어가게 둔다.
- 세션 임시 폴더(scratchpad)에만 있는 것 중 다시 필요한 것(스크립트, 측정 방법, 자료 메모)은 저장소로 옮긴다.

## 3. 확인

`git status --short`와 `git diff --stat`으로 문서가 바뀌었는지 본다. 커밋은 하지 않는다(요청할 때만).

## 4. 알리기

사용자에게 적은 곳을 짧게 알리고, 이어서 "`/rename <이름>` 뒤 `/clear`"를 권한다. 이름은 작업을 나타내게 제안한다(예: `13단계-질문`, `배포-1029`, `비용-조사`). 돌아올 때는 `/resume`.
