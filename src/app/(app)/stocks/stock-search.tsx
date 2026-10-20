"use client";

import { useActionState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { matchStocksAction, type MatchState } from "./actions";
import { CandidateItem } from "./candidate-item";

const initialState: MatchState = {
  searchId: 0,
  query: "",
  candidates: [],
  searched: false,
};

export function StockSearch() {
  const [state, formAction, pending] = useActionState(
    matchStocksAction,
    initialState,
  );

  return (
    <section className="mt-6">
      <form action={formAction} className="flex gap-2">
        <Label htmlFor="stock-query" className="sr-only">
          종목 이름 또는 티커
        </Label>
        <Input
          id="stock-query"
          name="query"
          placeholder="예: 삼전, 엔비디아, NVDA"
          maxLength={50}
          autoComplete="off"
          required
          defaultValue={state.query}
        />
        <Button type="submit" disabled={pending} className="shrink-0">
          {pending ? "찾는 중…" : "찾기"}
        </Button>
      </form>

      {state.error && (
        <p role="alert" className="mt-3 text-sm text-destructive">
          {state.error}
        </p>
      )}
      {state.searched && !state.error && state.candidates.length === 0 && (
        <p className="mt-3 text-sm text-muted-foreground">
          맞는 종목을 찾지 못했습니다. 다른 이름이나 티커로 입력해 보세요.
        </p>
      )}
      {state.candidates.length > 0 && (
        <ul
          aria-label="찾은 후보"
          className="mt-3 divide-y rounded-lg border"
          data-testid="candidate-list"
        >
          {state.candidates.map((candidate) => (
            <CandidateItem
              key={`${state.searchId}:${candidate.market}:${candidate.code}`}
              candidate={candidate}
              query={state.query}
            />
          ))}
        </ul>
      )}
    </section>
  );
}
