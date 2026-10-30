#!/bin/bash
# PostToolUse(Edit|Write): Claude가 고친 파일을 Prettier로 맞춘다(DECISIONS.md "개발 환경: 자동 서식").
# 프로젝트 밖 파일, Prettier가 모르는 형식, .prettierignore에 든 파일(*.md 등)은 건드리지 않는다.
file=$(jq -r '.tool_input.file_path // empty')
[ -n "$file" ] && [ -f "$file" ] || exit 0
case "$file" in
  "$CLAUDE_PROJECT_DIR"/*) ;;
  *) exit 0 ;;
esac
cd "$CLAUDE_PROJECT_DIR" || exit 0
node_modules/.bin/prettier --write --ignore-unknown --log-level warn "$file" >/dev/null 2>&1
exit 0
