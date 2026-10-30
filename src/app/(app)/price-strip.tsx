import Link from "next/link";
import { cn } from "cn";
import type { Market } from "@/server/db/schema";
import type { PriceStripItem } from "@/server/prices/store";
import { formatSeoulDateTime, formatTimelineTime } from "@/server/time";
import type { TimelineFilter } from "@/server/timeline/read";
import { timelineHref } from "./timeline-filters";

const PRICE_FORMAT: Record<Market, Intl.NumberFormat> = {
  KR: new Intl.NumberFormat("ko-KR", { maximumFractionDigits: 0 }),
  US: new Intl.NumberFormat("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }),
};

/**
 * 타임라인 맨 위 주가 띠(13단계, 2026-10-30 결정). 종목마다 현재가·전일 대비·등락률·기준 시각.
 * 한국 주식 관례대로 상승 빨강, 하락 파랑. 누르면 그 종목 기사만 보이고, 한 번 더 누르면 풀린다.
 * 자동 새로 고침은 없다(새로 고침하면 worker가 1분마다 받아 둔 최근 값).
 */
export function PriceStrip({
  items,
  filter,
  now,
}: {
  items: PriceStripItem[];
  filter: TimelineFilter;
  now: Date;
}) {
  if (items.length === 0) return null;
  return (
    <section className="mt-4" aria-label="주가" data-testid="price-strip">
      <ul className="-mx-4 flex snap-x gap-2 overflow-x-auto px-4 pb-2">
        {items.map((item) => {
          const selected = filter.stockId === item.stockId;
          const direction = Math.sign(item.change);
          const format = PRICE_FORMAT[item.market];
          return (
            <li key={item.stockId} className="shrink-0 snap-start">
              <Link
                href={timelineHref({
                  ...filter,
                  topicId: null,
                  stockId: selected ? null : item.stockId,
                })}
                scroll={false}
                aria-current={selected ? "true" : undefined}
                className={cn(
                  "flex min-w-36 flex-col gap-0.5 rounded-lg border px-3 py-2 transition-colors",
                  selected
                    ? "border-primary bg-primary/5"
                    : "hover:bg-muted/60",
                )}
                data-testid="price-item"
              >
                <span className="text-xs font-medium">{item.name}</span>
                <span className="text-base font-semibold tabular-nums">
                  {item.market === "US" && "$"}
                  {format.format(item.price)}
                  {item.market === "KR" && (
                    <span className="ml-0.5 text-xs font-normal">원</span>
                  )}
                </span>
                <span
                  className={cn(
                    "text-xs tabular-nums",
                    direction > 0 && "text-red-600 dark:text-red-400",
                    direction < 0 && "text-blue-600 dark:text-blue-400",
                    direction === 0 && "text-muted-foreground",
                  )}
                  data-testid="price-change"
                >
                  {direction > 0 ? "▲" : direction < 0 ? "▼" : "-"}
                  {format.format(Math.abs(item.change))}{" "}
                  {direction > 0 ? "+" : direction < 0 ? "-" : ""}
                  {Math.abs(item.changePercent).toFixed(2)}%
                </span>
                <span className="text-[11px] text-muted-foreground tabular-nums">
                  {item.open
                    ? `${formatSeoulDateTime(item.at).slice(11, 16)} 기준`
                    : `장 마감 · ${formatTimelineTime(item.at, now)} 기준`}
                </span>
              </Link>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
