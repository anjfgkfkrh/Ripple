"use client";

import { useActionState } from "react";
import { Button } from "@/components/ui/button";
import { collectNowAction, type CollectState } from "./actions";

/** 6단계 스케줄러 전까지 수집을 시작하는 방법(4단계 결정). */
export function CollectNowButton({ running }: { running: boolean }) {
  const [state, formAction, pending] = useActionState<CollectState>(
    collectNowAction,
    {},
  );

  return (
    <form action={formAction} className="flex flex-col items-end gap-1">
      <Button type="submit" size="sm" disabled={running || pending}>
        {running ? "수집 중…" : pending ? "요청 중…" : "지금 수집"}
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
