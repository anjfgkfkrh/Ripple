import * as z from "zod";
import type { Db } from "./db/client";
import { COLLECT_INTERVALS } from "./schedule/plan";
import { readSetting, writeSetting } from "./settings";
import { TOPIC_TIME } from "./topics/plan";

/** OpenRouter 모델 ID(예: anthropic/claude-haiku-5.5, ~typesafe/jev-latest) */
const MODEL_ID = /^[A-Za-z0-9~][A-Za-z0-9._:/~-]*$/;
/** 토픽 갱신 특정 시각은 이만큼까지 */
export const MAX_TOPIC_TIMES = 12;

/** 0~10 점수 기준(소수 첫째 자리까지) */
const score = () =>
  z.coerce
    .number({ message: "숫자를 넣으세요." })
    .min(0, "0~10 사이로 넣으세요.")
    .max(10, "0~10 사이로 넣으세요.")
    .transform((value) => Math.round(value * 10) / 10);

/** 설정 화면에서 바꿀 수 있는 항목(SPEC FR-8). Jev 모델·단가는 보이기만 한다(2026-10-21 결정). */
export const settingsFormSchema = z.object({
  collectIntervalMinutes: z.coerce
    .number()
    .refine(
      (value) => (COLLECT_INTERVALS as readonly number[]).includes(value),
      {
        message: "15·30·60·180분 중에서 고르세요.",
      },
    ),
  keywordsDepth: z.coerce
    .number()
    .int("1~4 사이 정수로 넣으세요.")
    .min(1, "1~4 사이로 넣으세요.")
    .max(4, "1~4 사이로 넣으세요."),
  keywordsRegenerateEvery: z.enum(["daily", "weekly"], {
    message: "하루 1회나 일주일 1회를 고르세요.",
  }),
  minRelevance: score(),
  /** 11단계: 기사는 관련도·영향도가 둘 다 기준 이상이어야 보인다 */
  minMagnitude: score(),
  /** 11단계: 키워드 후보를 남기고 토픽을 종목에 붙이는 기준 */
  keywordMinRelevance: score(),
  keywordMinMagnitude: score(),
  /** 11단계: 자동 토픽 갱신 간격(시간, 0은 끔)과 특정 시각(쉼표로 구분한 HH:MM) */
  topicIntervalHours: z.coerce
    .number({ message: "숫자를 넣으세요." })
    .int("0~24 사이 정수로 넣으세요.")
    .min(0, "0~24 사이로 넣으세요.")
    .max(24, "0~24 사이로 넣으세요."),
  topicTimes: z
    .string({ message: "09:00, 15:30처럼 넣으세요." })
    .max(200, "너무 깁니다.")
    .transform((value) =>
      value
        .split(/[\s,]+/)
        // 9:00처럼 시를 한 자리로 써도 받는다.
        .map((time) => time.trim().replace(/^(\d):/, "0$1:"))
        .filter(Boolean),
    )
    .refine((times) => times.every((time) => TOPIC_TIME.test(time)), {
      message: "09:00, 15:30처럼 24시간제 HH:MM으로 넣으세요.",
    })
    .refine((times) => new Set(times).size <= MAX_TOPIC_TIMES, {
      message: `${MAX_TOPIC_TIMES}개까지 넣을 수 있습니다.`,
    })
    .transform((times) => [...new Set(times)].sort()),
  maxPerRun: z.coerce
    .number({ message: "숫자를 넣으세요." })
    .int("정수로 넣으세요.")
    .min(10, "10~1000 사이로 넣으세요.")
    .max(1000, "10~1000 사이로 넣으세요."),
  monthlyBudgetUsd: z.coerce
    .number({ message: "숫자를 넣으세요." })
    .min(0.1, "$0.1~$1000 사이로 넣으세요.")
    .max(1000, "$0.1~$1000 사이로 넣으세요.")
    .transform((value) => Math.round(value * 100) / 100),
  llmModel: z
    .string()
    .trim()
    .min(1, "모델 ID를 넣으세요.")
    .max(200, "모델 ID가 너무 깁니다.")
    .regex(
      MODEL_ID,
      "모델 ID 형식이 아닙니다(예: anthropic/claude-haiku-5.5).",
    ),
});

export type SettingsForm = z.infer<typeof settingsFormSchema>;
export type SettingsField = keyof SettingsForm;

/** 지금 저장된 값(없으면 기본값)을 화면 형식으로 읽는다. */
export function readSettingsForm(db: Db): SettingsForm {
  return {
    collectIntervalMinutes: readSetting(db, "collect.intervalMinutes"),
    keywordsDepth: readSetting(db, "keywords.depth"),
    keywordsRegenerateEvery:
      readSetting(db, "keywords.regenerateEvery") === "weekly"
        ? "weekly"
        : "daily",
    minRelevance: readSetting(db, "evaluation.minRelevance"),
    minMagnitude: readSetting(db, "evaluation.minMagnitude"),
    keywordMinRelevance: readSetting(db, "keywords.minRelevance"),
    keywordMinMagnitude: readSetting(db, "keywords.minMagnitude"),
    topicIntervalHours: readSetting(db, "topics.intervalHours"),
    topicTimes: readSetting(db, "topics.times"),
    maxPerRun: readSetting(db, "evaluation.maxPerRun"),
    monthlyBudgetUsd: readSetting(db, "budget.monthlyUsd"),
    llmModel: readSetting(db, "llm.model"),
  };
}

/** 검사를 마친 값을 한 번에 저장한다(worker만 부른다, D-36). 바뀐 항목 이름을 돌려준다. */
export function writeSettingsForm(db: Db, form: SettingsForm): SettingsField[] {
  const before = readSettingsForm(db);
  db.transaction(() => {
    writeSetting(db, "collect.intervalMinutes", form.collectIntervalMinutes);
    writeSetting(db, "keywords.depth", form.keywordsDepth);
    writeSetting(db, "keywords.regenerateEvery", form.keywordsRegenerateEvery);
    writeSetting(db, "evaluation.minRelevance", form.minRelevance);
    writeSetting(db, "evaluation.minMagnitude", form.minMagnitude);
    writeSetting(db, "keywords.minRelevance", form.keywordMinRelevance);
    writeSetting(db, "keywords.minMagnitude", form.keywordMinMagnitude);
    writeSetting(db, "topics.intervalHours", form.topicIntervalHours);
    writeSetting(db, "topics.times", form.topicTimes);
    writeSetting(db, "evaluation.maxPerRun", form.maxPerRun);
    writeSetting(db, "budget.monthlyUsd", form.monthlyBudgetUsd);
    writeSetting(db, "llm.model", form.llmModel);
  });
  return (Object.keys(form) as SettingsField[]).filter(
    (key) => JSON.stringify(form[key]) !== JSON.stringify(before[key]),
  );
}

/** 입력칸에 보일 글자(특정 시각 목록은 쉼표로 잇는다) */
export function settingsFormText(
  form: SettingsForm,
): Record<SettingsField, string> {
  return Object.fromEntries(
    (Object.keys(form) as SettingsField[]).map((key) => {
      const value = form[key];
      return [key, Array.isArray(value) ? value.join(", ") : String(value)];
    }),
  ) as Record<SettingsField, string>;
}

/** zod 오류를 항목별 첫 메시지로 바꾼다(화면에 항목 옆에 보여 준다). */
export function settingsErrors(
  error: z.ZodError,
): Partial<Record<SettingsField, string>> {
  const errors: Partial<Record<SettingsField, string>> = {};
  for (const issue of error.issues) {
    const field = issue.path[0] as SettingsField | undefined;
    if (field && !errors[field]) errors[field] = issue.message;
  }
  return errors;
}
