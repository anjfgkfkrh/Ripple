import type { RunTrigger } from "@/server/db/schema";
import { startCollectionRun, type CollectDeps } from "./run";

export type CollectionRunner = {
  /** 수집을 시작하고 회차 id를 돌려준다. 이미 도는 중이면 null(같은 수집이 두 번 돌지 않게, SPEC FR-6). */
  start(trigger: RunTrigger): number | null;
  /** 지금 도는 회차 id */
  runningId(): number | null;
  /** 지금 도는 회차가 끝날 때까지 기다린다(테스트·종료용). */
  idle(): Promise<void>;
};

export function createCollectionRunner(deps: CollectDeps): CollectionRunner {
  let current: { runId: number; done: Promise<unknown> } | null = null;
  return {
    start(trigger) {
      if (current) return null;
      const run = startCollectionRun(deps, trigger);
      current = {
        runId: run.runId,
        done: run.done.finally(() => {
          current = null;
        }),
      };
      return run.runId;
    },
    runningId: () => current?.runId ?? null,
    async idle() {
      await current?.done;
    },
  };
}
