// 토픽 유지 개선(2026-10-30) 유료 비교: 지난 갱신 몇 회의 입력(제목·이전 토픽)을 지금 방식(옛)·새 방식으로 다시 돌린다.
// 실제 OpenRouter를 부른다(회당 약 $0.005~0.01). DB는 읽기 전용이고 사용량은 기록하지 않는다(결과의 비용을 사람이 옮긴다).
// 실행: README의 "유료 비교" 절차(esbuild로 묶어 시험용 Docker worker 안에서 돌림). 환경변수: DB_PATH, OPENROUTER_API_KEY, N(기본 4), COST_CAP(기본 0.15)
import { createOpenRouterClient } from "@/server/ai/llm/openrouter";
import { LlmError } from "@/server/ai/llm/types";
import type { LlmMessage } from "@/server/ai/llm/types";
import { openDb } from "@/server/db/client";
import { readSetting } from "@/server/settings";
import {
  buildTopicRefreshMessages,
  cleanTopicRefresh,
  MAX_TOPICS,
  topicRefreshSchema,
  type PreviousTopic,
  type RefreshedTopic,
} from "@/server/topics/refresh";
import {
  continueTopics,
  MAX_REFRESH_OUTPUT_TOKENS,
  retainDropped,
} from "@/server/topics/run";
import type { Topic } from "@/server/topics/store";

const HOUR = 3600e3;
const MAX_TITLES = 1500;
const dbPath = process.env.DB_PATH ?? "/tmp/check.db";
const count = Number(process.env.N ?? 4);
const costCap = Number(process.env.COST_CAP ?? 0.15);
const apiKey = process.env.OPENROUTER_API_KEY;
if (!apiKey) throw new Error("OPENROUTER_API_KEY가 없습니다.");

const db = openDb(dbPath, "readonly");
const sqlite = db.$client;
// 실패했을 때 원문을 볼 수 있게 마지막 응답 본문을 잡아 둔다.
let lastRaw = "";
const llm = createOpenRouterClient({
  apiKey,
  timeoutMs: 120_000,
  fetchImpl: async (url, init) => {
    const response = await fetch(url, init);
    lastRaw = await response.clone().text();
    return response;
  },
});
// ONLY=10-29 06:56,… 로 갱신 시각을 고르고, SKIP_OLD=1이면 옛 방식 호출을 건너뛴다(원인 조사용).
const only = process.env.ONLY?.split(",");
const model = readSetting(db, "llm.model");
const thresholds = {
  minRelevance: readSetting(db, "keywords.minRelevance"),
  minMagnitude: readSetting(db, "keywords.minMagnitude"),
};
const kst = (ms: number) =>
  new Date(ms + 9 * HOUR).toISOString().slice(5, 16).replace("T", " ");

type Row = {
  id: number;
  name: string;
  summary: string | null;
  terms: string;
  created_at: number;
  last_seen_at: number | null;
  ended_at: number | null;
};
const allTopics = sqlite
  .prepare(
    "select id,name,summary,terms,created_at,last_seen_at,ended_at from topics where source='auto' and deleted_at is null order by id",
  )
  .all() as Row[];

// 갱신 시각 = 자동 토픽의 만든·마지막 확인 시각이 모인 때
const times = [
  ...new Set(
    allTopics.flatMap((t) =>
      t.last_seen_at ? [t.created_at, t.last_seen_at] : [t.created_at],
    ),
  ),
].sort((a, b) => a - b);
const oldestTitle = (
  sqlite.prepare("select min(published_at) m from feed_headlines").get() as {
    m: number;
  }
).m;
const usable = times.filter(
  (t, i) => i > 0 && t - oldestTitle > 26 * HOUR && t - times[i - 1] > 0,
);
const step = Math.max(1, Math.floor(usable.length / count));
const picked = usable
  .slice(-step * count)
  .filter((_, i) => i % step === 0)
  .filter((t) => !only || only.includes(kst(t)));
console.log(
  `갱신 시각 ${times.length}개 중 쓸 수 있는 ${usable.length}개, 고른 것: ${picked.map(kst).join(", ")} (한국 시간), 모델 ${model}`,
);

const stockNames = (
  sqlite
    .prepare(
      "select name,name_ko from stocks where kind='stock' and deleted_at is null order by id",
    )
    .all() as { name: string; name_ko: string | null }[]
).map((s) => s.name_ko ?? s.name);

const countArticles = sqlite.prepare(
  `select s.topic_id topicId,
     count(distinct m.article_id) matched,
     count(distinct case when e.relevance>=@r and (e.magnitude is null or e.magnitude>=@g) then m.article_id end) shown
   from article_matches m
   join articles a on a.id=m.article_id
   join stocks s on s.id=m.stock_id
   left join evaluations e on e.match_id=m.id
   where s.kind='topic' and a.published_at>@since and a.published_at<=@until
   group by s.topic_id`,
);

/** 옛 프롬프트(HEAD 4c4f259 이전의 refresh.ts 그대로, 이전 토픽 줄에 기사 수·끝남 없음) */
function oldMessages(input: {
  previous: Omit<PreviousTopic, "articles" | "ended">[];
  titles: string[];
  items: string[];
}): LlmMessage[] {
  const [system, user] = buildTopicRefreshMessages({
    previous: [],
    titles: input.titles,
    items: input.items,
  });
  const content = system.content
    .replace(
      `최대 ${MAX_TOPICS}개 정한다. 그만큼 없으면 적게 정해도 된다.`,
      `10~${MAX_TOPICS}개 정한다.`,
    )
    .split("\n")
    .filter(
      (line) =>
        !line.startsWith("- 이전 토픽 뒤의 ") && !line.startsWith("- '끝남'이"),
    )
    .join("\n");
  if (content === system.content) throw new Error("옛 프롬프트를 만들지 못함");
  const userText = user.content.replace(
    "이전 토픽: (없음)",
    input.previous.length > 0
      ? [
          "이전 토픽(id: 이름 — 설명 [검색어]):",
          ...input.previous.map(
            (t) =>
              `- ${t.id}: ${t.name} — ${t.summary} [${t.terms.join(", ")}]`,
          ),
        ].join("\n")
      : "이전 토픽: (없음)",
  );
  return [
    { role: "system", content },
    { role: "user", content: userText },
  ];
}

const toTopic = (row: Row) =>
  ({
    id: row.id,
    name: row.name,
    terms: JSON.parse(row.terms) as string[],
  }) as Pick<Topic, "id" | "name" | "terms">;

let spent = 0;
let inputTokens = 0;
let outputTokens = 0;
const results: Record<string, unknown>[] = [];
const calls: { at: string; method: string; costUsd: number }[] = [];

for (const T of picked) {
  const prevT = times[times.indexOf(T) - 1];
  const since = Math.max(T - 24 * HOUR, prevT);
  const titles = (
    sqlite
      .prepare(
        "select title from feed_headlines where published_at>? and published_at<=? order by published_at desc, id desc limit ?",
      )
      .all(since, T, MAX_TITLES) as { title: string }[]
  ).map((r) => r.title);

  const active = allTopics.filter(
    (t) => t.created_at < T && (t.ended_at === null || t.ended_at >= T),
  );
  const recentlyEnded = allTopics.filter(
    (t) => t.ended_at !== null && t.ended_at < T && t.ended_at > T - 48 * HOUR,
  );
  const candidates = [...active, ...recentlyEnded];
  const counts = new Map(
    (
      countArticles.all({
        r: thresholds.minRelevance,
        g: thresholds.minMagnitude,
        since: T - 24 * HOUR,
        until: T,
      }) as { topicId: number; matched: number; shown: number }[]
    ).map((r) => [r.topicId, { matched: r.matched, shown: r.shown }]),
  );
  const countOf = (id: number) => counts.get(id) ?? { matched: 0, shown: 0 };
  const base = (t: Row) => ({
    id: String(t.id),
    name: t.name,
    summary: t.summary ?? "",
    terms: JSON.parse(t.terms) as string[],
  });

  const actual = {
    topics: allTopics.filter((t) => t.last_seen_at === T).length,
    created: allTopics.filter((t) => t.created_at === T).length,
    ended: allTopics.filter((t) => t.ended_at === T).length,
  };

  /** 실패(형식 오류 등)해도 이미 든 비용은 센다. 실패하면 null */
  async function call(method: string, messages: LlmMessage[]) {
    if (spent >= costCap) throw new Error(`비용 상한 ${costCap} 도달`);
    try {
      const { value, usage } = await llm.generateObject({
        model,
        messages,
        schema: topicRefreshSchema,
        schemaName: "topic_refresh",
        maxTokens: MAX_REFRESH_OUTPUT_TOKENS,
        timeoutMs: 120_000,
      });
      if (process.env.DUMP_RAW) {
        console.log(
          `  [${kst(T)} ${method}] 사용량 ${JSON.stringify((JSON.parse(lastRaw) as { usage?: unknown }).usage)}`,
        );
      }
      spent += usage.costUsd;
      inputTokens += usage.inputTokens;
      outputTokens += usage.outputTokens;
      calls.push({ at: kst(T), method, costUsd: usage.costUsd });
      return value;
    } catch (error) {
      const usage = error instanceof LlmError ? error.usage : undefined;
      spent += usage?.costUsd ?? 0;
      inputTokens += usage?.inputTokens ?? 0;
      outputTokens += usage?.outputTokens ?? 0;
      calls.push({ at: kst(T), method, costUsd: usage?.costUsd ?? 0 });
      if (process.env.DUMP_RAW) {
        const content = (
          JSON.parse(lastRaw) as {
            choices?: { message?: { content?: string } }[];
          }
        ).choices?.[0]?.message?.content;
        console.log(`RAW_HEAD ${content?.slice(0, 1500)}`);
        console.log(`RAW_TAIL ${content?.slice(-1500)}`);
        console.log(`RAW_LENGTH ${content?.length}`);
        console.log(
          `RAW_USAGE ${JSON.stringify((JSON.parse(lastRaw) as { usage?: unknown; choices?: { finish_reason?: string }[] }).usage)} finish=${(JSON.parse(lastRaw) as { choices?: { finish_reason?: string }[] }).choices?.[0]?.finish_reason}`,
        );
      }
      console.log(
        `  [${kst(T)} ${method}] 호출 실패: ${error instanceof Error ? error.message : error} (입력 ${usage?.inputTokens} 출력 ${usage?.outputTokens})`,
      );
      return null;
    }
  }

  // 옛 방식: 진행 중 토픽만 보여 주고, AI가 뺀 것은 끝낸다.
  const oldValue = process.env.SKIP_OLD
    ? null
    : await call(
        "옛",
        oldMessages({ previous: active.map(base), titles, items: stockNames }),
      );
  const oldRefreshed = oldValue
    ? continueTopics(
        cleanTopicRefresh(
          oldValue,
          new Set(active.map((t) => String(t.id))),
          stockNames,
        ),
        active.map(toTopic),
      )
    : null;
  const oldKept = new Set(
    (oldRefreshed ?? []).flatMap((t) => (t.id ? [Number(t.id)] : [])),
  );

  // 새 방식: 끝난 지 48시간 안의 토픽과 24시간 기사 수를 보여 주고, 뺀 토픽도 기사가 5건 이상이면 둔다.
  const newValue = await call(
    "새",
    buildTopicRefreshMessages({
      previous: candidates.map((t) => ({
        ...base(t),
        articles: countOf(t.id).matched,
        ended: recentlyEnded.includes(t),
      })),
      titles,
      items: stockNames,
    }),
  );
  const newRefreshed: RefreshedTopic[] | null = newValue
    ? continueTopics(
        cleanTopicRefresh(
          newValue,
          new Set(candidates.map((t) => String(t.id))),
          stockNames,
        ),
        candidates.map(toTopic),
      )
    : null;
  const newIds = new Set(
    (newRefreshed ?? []).flatMap((t) => (t.id ? [Number(t.id)] : [])),
  );
  const { carried, ended } = newRefreshed
    ? retainDropped(
        active
          .filter((t) => !newIds.has(t.id))
          .map((t) => ({ topic: toTopic(t) as Topic, counts: countOf(t.id) })),
        MAX_TOPICS - newRefreshed.length,
      )
    : { carried: [], ended: [] };
  const endedBefore = new Set(recentlyEnded.map((t) => t.id));

  const block = {
    시각: kst(T),
    제목: titles.length,
    "이전(진행/끝난48h)": `${active.length}/${recentlyEnded.length}`,
    "실제(그때)": `토픽 ${actual.topics} 새로 ${actual.created} 끝남 ${actual.ended}`,
    "옛(재실행)": oldRefreshed
      ? `토픽 ${oldRefreshed.length} 이어짐 ${oldKept.size} 새로 ${oldRefreshed.length - oldKept.size} 끝남 ${active.filter((t) => !oldKept.has(t.id)).length}`
      : "호출 실패",
    새: newRefreshed
      ? `토픽 ${newRefreshed.length + carried.length} 이어짐 ${newIds.size}(되살림 ${[...newIds].filter((id) => endedBefore.has(id)).length}) 유지 ${carried.length} 새로 ${newRefreshed.length - newIds.size} 끝남 ${ended.length}`
      : "호출 실패",
  };
  results.push(block);
  console.log(block);
  const nameOf = (id: number) => allTopics.find((t) => t.id === id)?.name ?? id;
  console.log(
    "  새 방식: 유지한 토픽 →",
    carried
      .map(
        (t) =>
          `${nameOf(t.id)}(${countOf(t.id).matched}건/통과 ${countOf(t.id).shown})`,
      )
      .join(", ") || "-",
  );
  console.log(
    "  새 방식: 끝낸 토픽 →",
    ended
      .map(
        (t) =>
          `${nameOf(t.id)}(${countOf(t.id).matched}건/통과 ${countOf(t.id).shown})`,
      )
      .join(", ") || "-",
  );
  console.log(
    "  옛 방식: 끝낸 토픽 →",
    active
      .filter((t) => oldRefreshed && !oldKept.has(t.id))
      .map(
        (t) =>
          `${t.name}(${countOf(t.id).matched}건/통과 ${countOf(t.id).shown})`,
      )
      .join(", ") || "-",
  );
}

console.table(results);
console.log(
  `호출 ${calls.length}회, 입력 ${inputTokens} 출력 ${outputTokens} 토큰, 비용 $${spent.toFixed(4)}`,
);
console.log("CALLS_JSON " + JSON.stringify(calls));
