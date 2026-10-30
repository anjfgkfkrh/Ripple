#!/bin/sh
# 본 DB를 읽기 전용으로 조회한다(worker 컨테이너 안 better-sqlite3 readonly).
# 사용: scripts/analysis/db-query.sh 질의.sql   (질의는 ;와 줄바꿈으로 나눔, 표로 출력)
#       docker compose exec -T worker node - < scripts/analysis/topic-sim.cjs  (분석 스크립트는 표준 입력으로)
SQL=$(cat "$1")
cd "$(dirname "$0")/../.." || exit 1
{ printf 'const SQL=%s;\n' "$(node -e 'process.stdout.write(JSON.stringify(require("fs").readFileSync(0,"utf8")))' <<<"$SQL")"; cat <<'JS'
const Database=require('better-sqlite3');const db=new Database('/data/ripple.db',{readonly:true,fileMustExist:true});
for (const s of SQL.split(/;\s*\n/).map(x=>x.trim()).filter(Boolean)){console.log('### '+s.split('\n')[0].slice(0,100));try{console.table(db.prepare(s).all().slice(0,200));}catch(e){console.log('ERR',e.message)}}
JS
} | docker compose exec -T worker node -
