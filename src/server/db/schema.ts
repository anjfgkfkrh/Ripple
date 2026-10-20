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
    createdAt: integer("created_at", { mode: "timestamp_ms" }).notNull(),
  },
  (table) => [index("keywords_stock").on(table.stockId)],
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
  },
  (table) => [index("collection_runs_started").on(table.startedAt)],
);
