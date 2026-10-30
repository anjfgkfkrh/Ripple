const Database=require('better-sqlite3');const db=new Database('/data/ripple.db',{readonly:true,fileMustExist:true});
const H=3600e3;
const FH=db.prepare("select title,summary,published_at p from feed_headlines").all();
const minP=Math.min(...FH.map(h=>h.p));
const norm=s=>s.toLowerCase().replace(/\s+/g,' ');
const words=s=>norm(s).split(' ').map(w=>w.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu,'')).filter(Boolean);
const AW=/^[a-z0-9][a-z0-9.&+'-]*$/;
const has=(h,w)=>AW.test(w)?new RegExp(`(?<![a-z0-9])${w.replace(/[.*+?^${}()|[\]\\]/g,'\\$&')}(?![a-z0-9])`).test(h):h.includes(w);
const X=FH.map(h=>({p:h.p,t:norm(h.title+' '+(h.summary||''))}));
const prevAt=db.prepare("select max(at) a from usage_logs where purpose='topic_refresh' and at < ? and input_tokens>0");
const P=new Map(db.prepare(`select s.topic_id tid, sum(e.relevance>=6 and e.magnitude>=6) pass from evaluations e join stocks s on s.id=e.stock_id where s.kind='topic' group by 1`).all().map(r=>[r.tid,r.pass]));
const ended=db.prepare("select id,name,terms,ended_at e from topics where source='auto' and status='ended' and ended_at >= ? order by ended_at").all(minP+24*H);
const cnt=(ws,a,b)=>X.filter(h=>h.p>=a&&h.p<b&&ws.some(w=>w.length&&w.every(x=>has(h.t,x)))).length;
const rows=[];
for(const t of ended){const ws=JSON.parse(t.terms).map(words);const last=prevAt.get(t.e).a;
 rows.push({name:t.name,seen:cnt(ws,last,t.e),day:cnt(ws,t.e-24*H,t.e),pass:P.get(t.id)||0});}
for(const r of rows) console.log(`${r.seen}\t${r.day}\tpass ${r.pass}\t${r.name}`);
const k=[1,3,5,10];for(const n of k) console.log(`24시간 제목 ${n}건 이상이었던 끝난 토픽: ${rows.filter(r=>r.day>=n).length}/${rows.length}`);
