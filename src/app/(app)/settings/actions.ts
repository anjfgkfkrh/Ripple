"use server";

import { refresh } from "next/cache";
import { requireSession } from "@/server/auth/guard";
import { describeStockError } from "@/server/stocks/messages";
import { webWorkerApi } from "@/server/web-worker";
import {
  SETTINGS_FIELDS,
  type SettingsFieldName,
  type SettingsValues,
} from "./fields";

export type SettingsState = {
  /** 입력칸에 다시 보여 줄 값(저장했으면 저장한 값, 틀렸으면 넣은 값) */
  values: SettingsValues;
  saved?: boolean;
  message?: string;
  errors?: Partial<Record<SettingsFieldName, string>>;
};

/** 설정 저장(SPEC FR-8): worker가 검사하고 저장한다(D-36). */
export async function saveSettingsAction(
  _previous: SettingsState,
  formData: FormData,
): Promise<SettingsState> {
  await requireSession();
  const values = Object.fromEntries(
    SETTINGS_FIELDS.map((field) => [field, String(formData.get(field) ?? "")]),
  ) as SettingsValues;
  const result = await webWorkerApi().updateSettings(values);
  if (result.ok) {
    refresh();
    return {
      values,
      saved: true,
      message:
        result.data.changed.length > 0
          ? "저장했습니다."
          : "바뀐 값이 없습니다.",
    };
  }
  switch (result.error) {
    case "invalid_settings":
      return {
        values,
        errors: result.body.fields as SettingsState["errors"],
        message: "고칠 항목이 있어 저장하지 않았습니다.",
      };
    case "unknown_model":
      return {
        values,
        errors: { llmModel: "OpenRouter에 없는 모델 ID입니다." },
        message: "고칠 항목이 있어 저장하지 않았습니다.",
      };
    case "model_check_failed":
      return {
        values,
        errors: {
          llmModel:
            "OpenRouter 모델 목록을 확인하지 못했습니다. 잠시 뒤 다시 저장하세요.",
        },
        message: "저장하지 않았습니다.",
      };
    default:
      return { values, message: describeStockError(result.error, result.body) };
  }
}

/** 저장한 깊이와 다르게 만든 종목의 키워드를 지금 다시 만든다(2026-10-21 결정). */
export async function rebuildKeywordsAction(): Promise<{ message: string }> {
  await requireSession();
  const result = await webWorkerApi().rebuildKeywords();
  if (!result.ok) {
    return { message: describeStockError(result.error, result.body) };
  }
  refresh();
  return {
    message:
      result.data.count > 0
        ? `${result.data.count}개 종목의 키워드를 다시 만들기 시작했습니다. 종목 화면에서 진행을 볼 수 있습니다.`
        : "다시 만들 종목이 없습니다.",
  };
}
