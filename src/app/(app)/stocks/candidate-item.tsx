"use client";

import { useActionState } from "react";
import { Button } from "@/components/ui/button";
import type { StockCandidate } from "@/server/stocks/match";
import { registerStockAction, type RegisterState } from "./actions";
import { StockMeta } from "./stock-meta";

export function CandidateItem(props: {
  candidate: StockCandidate;
  query: string;
}) {
  const { candidate } = props;
  const [state, formAction, pending] = useActionState<RegisterState, FormData>(
    registerStockAction,
    candidate.alreadyRegistered ? { status: "exists" } : {},
  );
  const displayName = candidate.nameKo ?? candidate.name;

  return (
    <li
      className="flex items-center justify-between gap-3 p-3"
      data-testid="candidate"
    >
      <div className="min-w-0">
        <p className="truncate font-medium">{displayName}</p>
        <StockMeta {...candidate} />
        {state.error && (
          <p role="alert" className="mt-1 text-xs text-destructive">
            {state.error}
          </p>
        )}
      </div>
      {state.status ? (
        <span className="shrink-0 text-sm text-muted-foreground">
          {state.status === "exists" ? "이미 등록됨" : "등록함"}
        </span>
      ) : (
        <form action={formAction} className="shrink-0">
          <input type="hidden" name="market" value={candidate.market} />
          <input type="hidden" name="code" value={candidate.code} />
          <input type="hidden" name="nameKo" value={candidate.nameKo ?? ""} />
          <input type="hidden" name="userInput" value={props.query} />
          <Button
            type="submit"
            size="sm"
            disabled={pending}
            aria-label={`${displayName} 등록`}
          >
            {pending ? "등록 중…" : "등록"}
          </Button>
        </form>
      )}
    </li>
  );
}
