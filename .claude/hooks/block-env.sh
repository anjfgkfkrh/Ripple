#!/bin/bash
# PreToolUse(Bash): 명령에 .env(비밀 값 파일)가 들어 있으면 막는다(DECISIONS.md "개발 환경: .env 막기").
# .env.example 같은 다른 이름은 그대로 둔다. Read 도구는 settings.json의 permissions.deny가 막는다.
if jq -e '.tool_input.command // "" | test("(^|[^A-Za-z0-9_.-])\\.env(?![A-Za-z0-9_.-])")' >/dev/null; then
  echo ".env는 비밀 값 파일이라 Bash로 읽거나 고치지 않는다. 비밀 값 검사는 node scripts/check-secrets.mjs. 커밋 메시지에 .env를 쓰려면 메시지를 파일로 넘긴다(git commit -F)." >&2
  exit 2
fi
exit 0
