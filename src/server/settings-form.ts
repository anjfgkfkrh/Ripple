import * as z from "zod";
import type { Db } from "./db/client";
import { COLLECT_INTERVALS } from "./schedule/plan";
import { readSetting, writeSetting } from "./settings";

/** OpenRouter 모델 ID(예: anthropic/claude-haiku-5.5, ~typesafe/jev-latest) */
const MODEL_ID = /^[A-Za-z0-9~][A-Za-z0-9._:/~-]*$/;

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
  minRelevance: z.coerce
    .number({ message: "숫자를 넣으세요." })
    .min(0, "0~10 사이로 넣으세요.")
    .max(10, "0~10 사이로 넣으세요.")
    .transform((value) => Math.round(value * 10) / 10),
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
    writeSetting(db, "evaluation.maxPerRun", form.maxPerRun);
    writeSetting(db, "budget.monthlyUsd", form.monthlyBudgetUsd);
    writeSetting(db, "llm.model", form.llmModel);
  });
  return (Object.keys(form) as SettingsField[]).filter(
    (key) => form[key] !== before[key],
  );
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
