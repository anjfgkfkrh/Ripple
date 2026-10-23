"use client";

import { useActionState } from "react";
import { Button } from "@/components/ui/button";
import { setMarketAction, type MarketState } from "./actions";

/** 한국·미국 증시 켜기/끄기(11단계). 끄면 항목을 숨기고, 모은 기사와 판정은 보관한다. */
export function MarketToggle(props: {
  market: "KR" | "US";
  name: string;
  enabled: boolean;
}) {
  const [state, formAction, pending] = useActionState<MarketState, FormData>(
    setMarketAction,
    {},
  );
  return (
    <form
      action={formAction}
      className="flex shrink-0 flex-col items-end gap-1"
    >
      <input type="hidden" name="market" value={props.market} />
      <input type="hidden" name="enabled" value={String(!props.enabled)} />
      <Button
        type="submit"
        variant={props.enabled ? "ghost" : "outline"}
        size="sm"
        disabled={pending}
        aria-label={`${props.name} ${props.enabled ? "끄기" : "켜기"}`}
      >
        {pending ? "요청 중…" : props.enabled ? "끄기" : "켜기"}
      </Button>
      {state.error && (
        <p role="alert" className="text-xs text-destructive">
          {state.error}
        </p>
      )}
    </form>
  );
}
