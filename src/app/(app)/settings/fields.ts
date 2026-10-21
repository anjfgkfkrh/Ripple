/** 설정 화면 입력 이름(서버의 settingsFormSchema 항목과 같다). 클라이언트에서도 쓰므로 서버 코드를 끌어오지 않는다. */
export const SETTINGS_FIELDS = [
  "collectIntervalMinutes",
  "keywordsDepth",
  "keywordsRegenerateEvery",
  "minRelevance",
  "maxPerRun",
  "monthlyBudgetUsd",
  "llmModel",
] as const;

export type SettingsFieldName = (typeof SETTINGS_FIELDS)[number];
export type SettingsValues = Record<SettingsFieldName, string>;
