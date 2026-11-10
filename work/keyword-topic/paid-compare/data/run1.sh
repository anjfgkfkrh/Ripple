#!/bin/sh
# 사용: run1.sh 이름... (이미 응답이 있으면 건너뜀)
for n in "$@"; do
  out=/work/res/or/$n.json
  [ -s "$out" ] && continue
  code=$(curl -sS --max-time 180 -X POST https://openrouter.ai/api/v1/chat/completions -H "Authorization: Bearer $OPENROUTER_API_KEY" -H 'Content-Type: application/json' --data-binary @/work/req/or/$n.json -o "$out" -w '%{http_code}')
  echo "$n: HTTP $code"
  [ "$code" = 200 ] || mv "$out" "$out.err$code"
done
