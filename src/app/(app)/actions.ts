"use server";

import { refresh } from "next/cache";
import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import { requireSession } from "@/server/auth/guard";
import { SESSION_COOKIE } from "@/server/auth/session";
import { describeStockError } from "@/server/stocks/messages";
import { MAX_READ_BATCH } from "@/server/timeline/reads";
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

export type TopicRefreshState = { error?: string; notice?: string };

/** 11단계 "지금 갱신": worker에 자동 토픽 갱신을 맡기고 바로 돌아온다. 진행은 상태 화면이 다시 읽어 보여 준다. */
export async function refreshTopicsAction(): Promise<TopicRefreshState> {
  await requireSession();
  const result = await webWorkerApi().refreshTopics();
  if (!result.ok)
    return { error: describeStockError(result.error, result.body) };
  refresh();
  return result.data.started ? {} : { notice: "이미 갱신 중입니다." };
}

export type ArticleReadState = { error?: string };

/**
 * 12단계 읽음 표시: 카드에 묶인 기사들을 읽음/안 읽음으로 저장한다.
 * 카드의 원문 링크를 누를 때와 카드의 작은 버튼에서 부른다. 인자는 믿지 않고 형식을 다시 본다.
 */
export async function setArticlesReadAction(
  articleIds: number[],
  read: boolean,
): Promise<ArticleReadState> {
  await requireSession();
  if (
    typeof read !== "boolean" ||
    !Array.isArray(articleIds) ||
    articleIds.length === 0 ||
    articleIds.length > MAX_READ_BATCH ||
    !articleIds.every((id) => Number.isInteger(id) && id > 0)
  ) {
    return { error: "읽음 표시를 저장하지 못했습니다." };
  }
  const result = await webWorkerApi().setArticlesRead(articleIds, read);
  if (!result.ok) {
    return { error: describeStockError(result.error, result.body) };
  }
  refresh();
  return {};
}
