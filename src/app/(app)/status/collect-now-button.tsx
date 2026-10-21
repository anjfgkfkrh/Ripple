"use client";

import { useActionState } from "react";
import { Button } from "@/components/ui/button";
import { collectNowAction, type CollectState } from "../actions";

/** 정기 수집을 기다리지 않고 바로 수집한다(4단계 결정, D-29 결정 전까지 상태 화면에 둔다). */
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
