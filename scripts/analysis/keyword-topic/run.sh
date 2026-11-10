#!/bin/sh
# 시험용 컨테이너 안에서 돈다. /work/req/<종류>/*.json 을 차례로 보내고 /work/res/<종류>/ 에 응답을 둔다.
# 키는 컨테이너 환경 변수에서만 읽고 출력하지 않는다. 이미 응답이 있는 요청은 건너뛴다.
kind="$1"
ok=0
bad=0
for f in /work/req/"$kind"/*.json; do
  n=$(basename "$f")
  out=/work/res/"$kind"/"$n"
  [ -s "$out" ] && continue
  if [ "$kind" = jev ]; then
    code=$(curl -sS --max-time 20 -X POST https://api.typesafe.ai/v1/systemone \
      -H "Authorization: Bearer $TYPESAFE_API_KEY" -H 'Content-Type: application/json' -H 'Accept: application/json' \
      --data-binary @"$f" -o "$out" -w '%{http_code}')
  else
    code=$(curl -sS --max-time 180 -X POST https://openrouter.ai/api/v1/chat/completions \
      -H "Authorization: Bearer $OPENROUTER_API_KEY" -H 'Content-Type: application/json' \
      --data-binary @"$f" -o "$out" -w '%{http_code}')
  fi
  if [ "$code" = 200 ]; then
    ok=$((ok + 1))
  else
    bad=$((bad + 1))
    mv "$out" "$out.err$code" 2>/dev/null
    echo "$n: HTTP $code"
    [ "$bad" -ge 5 ] && echo "실패가 5번이라 멈춤" && break
  fi
done
echo "성공 $ok, 실패 $bad"
