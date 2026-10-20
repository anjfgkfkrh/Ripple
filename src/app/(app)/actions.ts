"use server";

import { refresh } from "next/cache";
import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import { requireSession } from "@/server/auth/guard";
import { SESSION_COOKIE } from "@/server/auth/session";
import { describeStockError } from "@/server/stocks/messages";
import { webWorkerApi } from "@/server/web-worker";

export async function logout(): Promise<void> {
  (await cookies()).delete(SESSION_COOKIE);
  redirect("/login");
}

export type CollectState = { error?: string; notice?: string };

/** "지금 수집": worker에 수집을 맡기고 바로 돌아온다. 진행 상황은 화면이 다시 읽어 보여 준다. */
export async function collectNowAction(): Promise<CollectState> {
  await requireSession();
  const result = await webWorkerApi().startCollection();
  if (!result.ok) {
    if (result.error === "already_running") {
      refresh();
      return { notice: "이미 수집 중입니다." };
    }
    return { error: describeStockError(result.error, result.body) };
  }
  refresh();
  return {};
}
