import { eq } from "drizzle-orm";
import type { Db } from "./db/client";
import { settings } from "./db/schema";

/** 설정 기본값 (SPEC FR-8). 설정 화면(8단계)에서 바꾼 값은 settings 테이블에 남는다. */
export const SETTING_DEFAULTS: {
  /** 종목 매칭·키워드 생성 모델 (D-02) */
  "llm.model": string;
  /** 월 예산 한도, 달러 (D-04 임시) */
  "budget.monthlyUsd": number;
  /** 키워드 확장 깊이 1~4 (SPEC FR-3, 기본 2) */
  "keywords.depth": number;
  /** 기사 평가 모델. 버전을 고정한다(D-23) */
  "jev.model": string;
  /** Jev 단가: 입력 100만 토큰당 달러(D-06, 2026-10-20 공식 가격). 출력은 무료 */
  "jev.usdPerMillionInputTokens": number;
  /** 화면에 보일 관련도 기준 0~10 (SPEC FR-5, 기본 6) */
  "evaluation.minRelevance": number;
  /** 수집 회차 하나에서 평가할 최대 연결 수 (D-03, 기본 200) */
  "evaluation.maxPerRun": number;
  /** 정기 수집 간격(분): 15·30·60·180 (SPEC FR-6, D-01 기본 30) */
  "collect.intervalMinutes": number;
  /** 키워드 자동 재생성 주기 (SPEC FR-3, 기본 하루 1회) */
  "keywords.regenerateEvery": "daily" | "weekly";
  /** 키워드 자동 재생성을 마지막으로 한 시각(ms). 0이면 아직 없음 */
  "keywords.autoRegeneratedAt": number;
  /** 공식 종목 목록을 마지막으로 받은 시각(ms). 0이면 아직 없음 */
  "listings.syncedAt.KR": number;
  "listings.syncedAt.US": number;
} = {
  "llm.model": "anthropic/claude-haiku-5.5",
  "budget.monthlyUsd": 5,
  "keywords.depth": 2,
  "jev.model": "jev-1.13.0",
  "jev.usdPerMillionInputTokens": 0.042,
  "evaluation.minRelevance": 6,
  "evaluation.maxPerRun": 200,
  "collect.intervalMinutes": 30,
  "keywords.regenerateEvery": "daily",
  "keywords.autoRegeneratedAt": 0,
  "listings.syncedAt.KR": 0,
  "listings.syncedAt.US": 0,
};

export type SettingKey = keyof typeof SETTING_DEFAULTS;

/** 저장된 값이 없거나 형식이 다르면 기본값을 쓴다. */
export function readSetting<K extends SettingKey>(
  db: Db,
  key: K,
): (typeof SETTING_DEFAULTS)[K] {
  const fallback = SETTING_DEFAULTS[key];
  const row = db.select().from(settings).where(eq(settings.key, key)).get();
  if (!row) return fallback;
  try {
    const value: unknown = JSON.parse(row.value);
    return typeof value === typeof fallback
      ? (value as (typeof SETTING_DEFAULTS)[K])
      : fallback;
  } catch {
    return fallback;
  }
}

/** worker만 부른다(web은 DB에 쓰지 않음, D-36). */
export function writeSetting<K extends SettingKey>(
  db: Db,
  key: K,
  value: (typeof SETTING_DEFAULTS)[K],
): void {
  const json = JSON.stringify(value);
  db.insert(settings)
    .values({ key, value: json })
    .onConflictDoUpdate({
      target: settings.key,
      set: { value: json, updatedAt: new Date() },
    })
    .run();
}
