import {
  index,
  integer,
  primaryKey,
  real,
  sqliteTable,
  text,
  uniqueIndex,
  type AnySQLiteColumn,
} from "drizzle-orm/sqlite-core";

export const MARKETS = ["KR", "US"] as const;
export type Market = (typeof MARKETS)[number];

/** 키워드 생성 상태 (D-37: 등록 직후 worker가 비동기로 만든다) */
export const KEYWORD_STATUSES = [
  "pending",
  "generating",
  "ready",
  "failed",
] as const;
export type KeywordStatus = (typeof KEYWORD_STATUSES)[number];

/**
 * 키워드 종류 (10단계, 2026-10-22 결정): 구조(제품·사업·고객 등 잘 바뀌지 않는 요인) /
 * 시사(지금 시장이 주가에 반영하는 기대·우려와 그것을 움직이는 사건·인물의 사슬)
 */
export const KEYWORD_KINDS = ["structure", "current"] as const;
export type KeywordKind = (typeof KEYWORD_KINDS)[number];

/** 설정 키-값 (SPEC FR-8). value는 JSON 문자열. */
export const settings = sqliteTable("settings", {
  key: text("key").primaryKey(),
  value: text("value").notNull(),
  // 시각은 UTC 기준 밀리초 정수로 저장하고, 화면에서 한국 시간으로 바꿔 보여 준다.
  updatedAt: integer("updated_at", { mode: "timestamp_ms" })
    .notNull()
    .$defaultFn(() => new Date())
    .$onUpdate(() => new Date()),
});

/**
 * 등록한 종목 (SPEC FR-2). 소유자 개념 없이 하나의 목록(D-38).
 * 삭제하면 지우지 않고 deletedAt만 채워 숨긴다(D-11). 다시 등록하면 되살린다.
 */
export const stocks = sqliteTable(
  "stocks",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    market: text("market", { enum: MARKETS }).notNull(),
    /** 한국: 종목코드(6자리, 영문이 섞일 수 있음), 미국: 티커 */
    code: text("code").notNull(),
    /** 공식 목록의 정식 이름 */
    name: text("name").notNull(),
    /** 한글 이름. 한국 종목은 정식 이름, 미국 종목은 LLM이 준 이름(예: 엔비디아) */
    nameKo: text("name_ko"),
    /** KOSPI·KOSDAQ·KONEX, NASDAQ·NYSE 등 */
    exchange: text("exchange").notNull(),
    /** 사용자가 처음 입력한 그대로 */
    userInput: text("user_input").notNull(),
    createdAt: integer("created_at", { mode: "timestamp_ms" })
      .notNull()
      .$defaultFn(() => new Date()),
    deletedAt: integer("deleted_at", { mode: "timestamp_ms" }),
    keywordsStatus: text("keywords_status", { enum: KEYWORD_STATUSES })
      .notNull()
      .default("pending"),
    /** 마지막 생성이 실패한 이유(화면 표시용) */
    keywordsError: text("keywords_error"),
    keywordsGeneratedAt: integer("keywords_generated_at", {
      mode: "timestamp_ms",
    }),
    /**
     * 새로 등록하거나 되살린 종목은 다음 회차에서 이 시각(등록 24시간 전) 이후 기사까지 연결한다.
     * 그 회차가 끝나면(완료·일부 실패) 비운다(2026-10-21 결정, D-20 보완).
     */
    backfillFrom: integer("backfill_from", { mode: "timestamp_ms" }),
  },
  (table) => [uniqueIndex("stocks_market_code").on(table.market, table.code)],
);

/**
 * 종목별 연관 키워드 트리 (SPEC FR-3). 최신 회차만 둔다(D-16).
 * 단계마다 총 5개, 2단계부터는 바로 윗단계 키워드 하나에 이어진다(D-13).
 */
export const keywords = sqliteTable(
  "keywords",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    stockId: integer("stock_id")
      .notNull()
      .references(() => stocks.id),
    /** 1~4 */
    level: integer("level").notNull(),
    parentId: integer("parent_id").references(
      (): AnySQLiteColumn => keywords.id,
      { onDelete: "cascade" },
    ),
    textKo: text("text_ko").notNull(),
    /** 미국 종목만: 영어 뉴스 검색용 표기 (D-14) */
    textEn: text("text_en"),
    /** 위 단계(1단계는 종목)와 어떻게 이어지는지 한 줄 */
    reason: text("reason").notNull(),
    /** 종목에서 이 키워드까지의 트리 경로(예: 삼성전자 → 메모리 → 엔비디아) */
    path: text("path").notNull(),
    /** 구조/시사. 시사 가지는 1단계 키워드의 종류를 따른다(10단계) */
    kind: text("kind", { enum: KEYWORD_KINDS }).notNull().default("structure"),
    createdAt: integer("created_at", { mode: "timestamp_ms" }).notNull(),
  },
  (table) => [index("keywords_stock").on(table.stockId)],
);

/**
 * 수집한 피드의 기사 제목 (10단계, 2026-10-22 결정). 종목에 걸리지 않은 기사도 제목·언론사·피드·시각만
 * 3일 동안 둔다(요약·본문 없음). 하루 한 번 시장 이슈를 정리하는 재료다.
 */
export const feedHeadlines = sqliteTable(
  "feed_headlines",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    /** 같은 제목은 한 줄(공백·대소문자를 정리한 제목) */
    titleKey: text("title_key").notNull(),
    title: text("title").notNull(),
    publisher: text("publisher").notNull(),
    /** 처음 받은 피드 id(언론사·분야, 예: rss:sbs:international) */
    feed: text("feed").notNull(),
    /** 피드에 발행 시각이 없으면 처음 받은 시각 */
    publishedAt: integer("published_at", { mode: "timestamp_ms" }).notNull(),
    seenAt: integer("seen_at", { mode: "timestamp_ms" }).notNull(),
  },
  (table) => [
    uniqueIndex("feed_headlines_title_key").on(table.titleKey),
    index("feed_headlines_published").on(table.publishedAt),
  ],
);

/** 시장 이슈 하나: 이름, 한 줄 설명, 기사에 쓰인 낱말 */
export type MarketIssue = { name: string; summary: string; terms: string[] };

/**
 * 하루 한 번 정리한 오늘의 시장 이슈 (10단계). 모든 종목의 키워드 트리를 만들 때 함께 넣는다.
 * 가장 최근 것만 쓰고, 지난 것은 확인용으로 잠시 둔다.
 */
export const marketIssues = sqliteTable(
  "market_issues",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    createdAt: integer("created_at", { mode: "timestamp_ms" }).notNull(),
    /** 정리에 넣은 제목 수 */
    headlineCount: integer("headline_count").notNull(),
    issues: text("issues", { mode: "json" }).$type<MarketIssue[]>().notNull(),
  },
  (table) => [index("market_issues_created").on(table.createdAt)],
);

/** 공식 종목 목록 (D-12). worker가 주 1회 통째로 갈아 끼운다. */
export const listings = sqliteTable(
  "listings",
  {
    market: text("market", { enum: MARKETS }).notNull(),
    code: text("code").notNull(),
    name: text("name").notNull(),
    exchange: text("exchange").notNull(),
    isEtf: integer("is_etf", { mode: "boolean" }).notNull().default(false),
  },
  (table) => [
    primaryKey({ columns: [table.market, table.code] }),
    index("listings_market_name").on(table.market, table.name),
  ],
);

/** AI 호출 기록 (SPEC FR-9). 한 줄이 호출 한 번이다. */
export const usageLogs = sqliteTable(
  "usage_logs",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    at: integer("at", { mode: "timestamp_ms" }).notNull(),
    provider: text("provider", { enum: ["openrouter", "typesafe"] }).notNull(),
    model: text("model").notNull(),
    /** 무엇에 썼는지(예: stock_match) */
    purpose: text("purpose").notNull(),
    inputTokens: integer("input_tokens").notNull(),
    outputTokens: integer("output_tokens").notNull(),
    /** 실제 청구액(달러). OpenRouter는 응답의 usage.cost (D-06) */
    costUsd: real("cost_usd").notNull(),
  },
  (table) => [index("usage_logs_at").on(table.at)],
);

/** 기사 유형. 리포트는 수집하지 않지만(4단계 결정) 나중을 위해 값은 둔다. */
export const ARTICLE_TYPES = ["news", "report"] as const;
export type ArticleType = (typeof ARTICLE_TYPES)[number];

/**
 * 수집한 기사 (SPEC FR-4). 종목에 하나라도 걸린 기사만 저장한다.
 * 같은 원문 주소는 한 기사로 합친다(D-19). 본문은 저장하지 않고 요약만 둔다.
 */
export const articles = sqliteTable(
  "articles",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    /** 중복 판정용으로 정리한 주소(스킴·추적 파라미터·끝 슬래시 제거) */
    urlKey: text("url_key").notNull(),
    /** 원문(또는 소스가 준) 주소 */
    url: text("url").notNull(),
    title: text("title").notNull(),
    /** 피드가 준 요약(300자까지) */
    summary: text("summary"),
    /** 언론사 또는 소스가 알려 준 출처(예: 조선비즈, Yahoo) */
    publisher: text("publisher").notNull(),
    type: text("type", { enum: ARTICLE_TYPES }).notNull().default("news"),
    /** 처음 받은 피드 id(예: rss:chosunbiz:stock, finnhub:company) */
    feed: text("feed").notNull(),
    /** 피드에 발행 시각이 없으면 처음 받은 시각 */
    publishedAt: integer("published_at", { mode: "timestamp_ms" }).notNull(),
    collectedAt: integer("collected_at", { mode: "timestamp_ms" }).notNull(),
  },
  (table) => [
    uniqueIndex("articles_url_key").on(table.urlKey),
    index("articles_published").on(table.publishedAt),
  ],
);

/** 기사가 종목에 걸린 방식: 소스가 종목을 직접 지정 / 종목 이름 / 키워드 */
export const MATCH_KINDS = ["direct", "name", "keyword"] as const;
export type MatchKind = (typeof MATCH_KINDS)[number];

/**
 * 기사–종목 연결. 종목마다 가장 가까운 경로 하나만 둔다.
 * 키워드를 다시 만들어도 남도록 당시 경로를 글자로 복사한다(D-16).
 */
export const articleMatches = sqliteTable(
  "article_matches",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    articleId: integer("article_id")
      .notNull()
      .references(() => articles.id),
    stockId: integer("stock_id")
      .notNull()
      .references(() => stocks.id),
    /** 예: 삼성전자 → HBM 메모리 → 엔비디아. 종목 자체로 걸리면 종목 이름 */
    keywordPath: text("keyword_path").notNull(),
    /** 걸린 키워드가 윗단계와 이어지는 이유(키워드 연결일 때만, 5단계 Jev 입력) */
    keywordReason: text("keyword_reason"),
    matchKind: text("match_kind", { enum: MATCH_KINDS }).notNull(),
    /** 0이면 종목 자체, 1~4는 키워드 단계 */
    level: integer("level").notNull(),
    createdAt: integer("created_at", { mode: "timestamp_ms" }).notNull(),
  },
  (table) => [
    uniqueIndex("article_matches_article_stock").on(
      table.articleId,
      table.stockId,
    ),
    index("article_matches_stock").on(table.stockId),
  ],
);

export const RUN_STATUSES = [
  "running",
  "success",
  "partial",
  "failed",
] as const;
export type RunStatus = (typeof RUN_STATUSES)[number];
/** 실행 사유: 직접 실행 / 정기 / 놓친 회차 보정(6단계) */
export const RUN_TRIGGERS = ["manual", "scheduled", "catch_up"] as const;
export type RunTrigger = (typeof RUN_TRIGGERS)[number];

/** 소스 하나의 수집 결과 */
export type SourceResult = {
  /** 피드 id */
  id: string;
  /** 화면 표시용(예: 조선비즈 증권) */
  label: string;
  /** 화면에서 묶어 보여 줄 출처(예: 조선비즈, Finnhub) */
  group: string;
  /** 받은 기사 수 */
  fetched: number;
  /** 기준 시각 이후 기사 수 */
  fresh: number;
  /** 새로 저장한 기사 수 */
  saved: number;
  /** 새로 만든 기사–종목 연결 수 */
  matched: number;
  /** 이 소스만 회차 기준 시각과 다른 기준 시각(ms)을 썼을 때(지난번에 실패한 소스, D-24) */
  since?: number;
  error?: string;
  ms: number;
};

/** 수집 회차 기록 (SPEC 7장 collection_runs). 6단계 누락 보정이 마지막 성공 시각을 쓴다. */
export const collectionRuns = sqliteTable(
  "collection_runs",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    trigger: text("trigger", { enum: RUN_TRIGGERS }).notNull(),
    status: text("status", { enum: RUN_STATUSES }).notNull(),
    startedAt: integer("started_at", { mode: "timestamp_ms" }).notNull(),
    finishedAt: integer("finished_at", { mode: "timestamp_ms" }),
    /** 이 시각보다 오래된 기사는 건너뛴다(D-20) */
    since: integer("since", { mode: "timestamp_ms" }).notNull(),
    sources: text("sources", { mode: "json" })
      .$type<SourceResult[]>()
      .notNull()
      .default([]),
    articlesNew: integer("articles_new").notNull().default(0),
    matchesNew: integer("matches_new").notNull().default(0),
    /** 회차 전체가 실패했거나 건너뛴 이유 */
    note: text("note"),
    /** 이 회차의 Jev 평가 결과(5단계). 평가를 하지 않았으면 null */
    evaluation: text("evaluation", { mode: "json" }).$type<EvaluationSummary>(),
  },
  (table) => [index("collection_runs_started").on(table.startedAt)],
);

/** 수집 회차 안에서 한 Jev 평가 요약 */
export type EvaluationSummary = {
  /** 새로 평가한 기사–종목 연결 수 */
  evaluated: number;
  /** 그중 표시 기준(관련도) 이상 */
  relevant: number;
  /** 실패해 다음 회차로 미룬 수 */
  failed: number;
  /** 예산·키 문제 등으로 손대지 않고 남긴 수 */
  remaining: number;
  /** 이 회차 평가 비용(달러) */
  costUsd: number;
  ms: number;
  /** 멈춘 이유(예산 초과, 키 없음 등) */
  note?: string;
};

/** 호재 / 악재 / 중립 (SPEC FR-5) */
export const IMPACTS = ["positive", "negative", "neutral"] as const;
export type Impact = (typeof IMPACTS)[number];

/**
 * Jev 평가(SPEC FR-5). 기사–종목 연결 하나에 한 줄.
 * 표시 기준 미만도 지우지 않고 두고 화면에서만 숨긴다(D-22).
 */
export const evaluations = sqliteTable(
  "evaluations",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    matchId: integer("match_id")
      .notNull()
      .references(() => articleMatches.id),
    articleId: integer("article_id")
      .notNull()
      .references(() => articles.id),
    stockId: integer("stock_id")
      .notNull()
      .references(() => stocks.id),
    /** 관련도 0~10(상황 6단계 기댓값 × 2) */
    relevance: real("relevance").notNull(),
    relevanceConfidence: real("relevance_confidence").notNull(),
    impact: text("impact", { enum: IMPACTS }).notNull(),
    impactConfidence: real("impact_confidence").notNull(),
    impactProbabilities: text("impact_probabilities", { mode: "json" })
      .$type<Record<Impact, number>>()
      .notNull(),
    /** 응답한 실제 모델 버전(D-23) */
    model: text("model").notNull(),
    evaluatedAt: integer("evaluated_at", { mode: "timestamp_ms" }).notNull(),
  },
  (table) => [
    uniqueIndex("evaluations_match").on(table.matchId),
    index("evaluations_stock_relevance").on(table.stockId, table.relevance),
  ],
);
