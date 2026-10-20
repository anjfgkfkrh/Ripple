"use client";

import { useActionState } from "react";
import { Button } from "@/components/ui/button";
import { regenerateKeywordsAction, type RegenerateState } from "./actions";

/** 키워드를 통째로 새로 만든다(D-15). 실패해도 이전 키워드는 남는다. */
export function RegenerateKeywordsButton(props: {
  id: number;
  name: string;
  className?: string;
}) {
  const [state, formAction, pending] = useActionState<
    RegenerateState,
    FormData
  >(regenerateKeywordsAction, {});

  return (
    <form action={formAction} className={props.className}>
      <input type="hidden" name="id" value={props.id} />
      <Button
        type="submit"
        variant="outline"
        size="xs"
        disabled={pending}
        aria-label={`${props.name} 키워드 다시 만들기`}
      >
        {pending ? "요청 중…" : "키워드 다시 만들기"}
      </Button>
      {state.error && (
        <p role="alert" className="mt-1 text-xs text-destructive">
          {state.error}
        </p>
      )}
    </form>
  );
}
