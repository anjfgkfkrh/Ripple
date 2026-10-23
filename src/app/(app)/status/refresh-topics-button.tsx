"use client";

import { useActionState } from "react";
import { Button } from "@/components/ui/button";
import { refreshTopicsAction, type TopicRefreshState } from "../actions";

/** 정해진 때를 기다리지 않고 자동 토픽을 지금 갱신한다(11단계). */
export function RefreshTopicsButton({ running }: { running: boolean }) {
  const [state, formAction, pending] = useActionState<TopicRefreshState>(
    refreshTopicsAction,
    {},
  );

  return (
    <form action={formAction} className="flex flex-col items-end gap-1">
      <Button type="submit" size="sm" disabled={running || pending}>
        {running ? "갱신 중…" : pending ? "요청 중…" : "지금 갱신"}
      </Button>
      {state.error && (
        <p role="alert" className="text-sm text-destructive">
          {state.error}
        </p>
      )}
      {state.notice && (
        <p className="text-sm text-muted-foreground">{state.notice}</p>
      )}
    </form>
  );
}
