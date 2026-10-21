"use client";

import { useActionState } from "react";
import { Button } from "@/components/ui/button";
import { rebuildKeywordsAction } from "./actions";

/** 저장한 깊이와 다르게 만든 종목이 있을 때만 켜진다(2026-10-21 결정). */
export function RebuildKeywordsButton({ count }: { count: number }) {
  const [state, formAction, pending] = useActionState<
    { message?: string },
    FormData
  >(rebuildKeywordsAction, {});

  return (
    <form action={formAction} className="flex flex-col items-start gap-2">
      <Button
        type="submit"
        variant="outline"
        size="sm"
        disabled={count === 0 || pending}
      >
        {pending ? "요청 중…" : "지금 다시 만들기"}
      </Button>
      {state.message && (
        <p className="text-xs text-muted-foreground">{state.message}</p>
      )}
    </form>
  );
}
