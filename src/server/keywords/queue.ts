import type { Decider } from "@/server/ai/decider/types";
import type { LlmClient } from "@/server/ai/llm/types";
import type { Db } from "@/server/db/client";
import { log, logError } from "@/server/log";
import { generateKeywordsForStock, setKeywordStatus } from "./generate";

export type KeywordQueue = {
  /** 생성 대기열에 넣는다. 이미 기다리거나 만드는 중인 종목이면 무시한다. 던지지 않는다. */
  enqueue(stockId: number): void;
  /** 대기열이 빌 때까지 기다린다(테스트·종료용). */
  idle(): Promise<void>;
};

/**
 * worker 안의 키워드 생성 대기열(D-37). 한 번에 하나씩 만들어 LLM 호출이 몰리지 않게 한다.
 * 등록 응답은 기다리지 않고 바로 돌아간다.
 */
export function createKeywordQueue(deps: {
  db: Db;
  llm: LlmClient;
  /** 후보 키워드를 관련도·영향도로 고르는 Jev(11단계). 키가 없으면 null(자르지 않음) */
  decider?: Decider | null;
  now?: () => Date;
}): KeywordQueue {
  const waiting: number[] = [];
  const queued = new Set<number>();
  let current: number | null = null;
  let running: Promise<void> | null = null;

  async function drain() {
    while (waiting.length > 0) {
      const stockId = waiting.shift()!;
      queued.delete(stockId);
      current = stockId;
      const started = Date.now();
      try {
        const result = await generateKeywordsForStock(
          deps.db,
          deps.llm,
          stockId,
          {
            now: deps.now,
            decider: deps.decider ?? null,
          },
        );
        const seconds = ((Date.now() - started) / 1000).toFixed(1);
        if (result.ok) {
          const dropped =
            result.dropped > 0
              ? `, 규칙에 안 맞아 버림 ${result.dropped}개`
              : "";
          const headlines =
            result.headlines > 0
              ? `, 기사 제목 ${result.headlines}개 참고`
              : "";
          const rated = result.unratedReason
            ? `, 평가 못 해 자르지 않음: ${result.unratedReason}`
            : `, Jev 평가 ${result.rated}개`;
          log(
            `키워드 생성: 종목 id ${stockId}, 후보 ${result.candidates}개 → ${result.count}개(깊이 ${result.depth}${dropped}${rated}${headlines}), ${seconds}초`,
          );
        } else {
          log(
            `키워드 생성 실패: 종목 id ${stockId}, ${result.error}, ${seconds}초`,
          );
        }
      } catch (error) {
        logError(`키워드 생성 중 예기치 못한 오류: 종목 id ${stockId}`, error);
      } finally {
        current = null;
      }
    }
  }

  return {
    enqueue(stockId) {
      if (queued.has(stockId) || current === stockId) return;
      try {
        setKeywordStatus(deps.db, stockId, "pending");
      } catch (error) {
        // 상태 표시만 늦을 뿐, 생성은 그대로 진행한다.
        logError(`키워드 대기 상태 기록 실패: 종목 id ${stockId}`, error);
      }
      queued.add(stockId);
      waiting.push(stockId);
      running ??= drain().finally(() => {
        running = null;
      });
    },
    idle() {
      return running ?? Promise.resolve();
    },
  };
}
