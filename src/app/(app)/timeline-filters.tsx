import Link from "next/link";
import type { ReactNode } from "react";
import { cn } from "cn";
import { IMPACTS } from "@/server/db/schema";
import {
  TIMELINE_PAGE_SIZE,
  type TimelineFilter,
} from "@/server/timeline/read";
import { IMPACT_LABEL } from "./impact";

/** 필터와 개수를 담은 타임라인 주소. 필터를 바꾸면 개수는 처음(30개)으로 돌아간다. */
export function timelineHref(filter: TimelineFilter, limit?: number): string {
  const params = new URLSearchParams();
  if (filter.stockId !== null) params.set("stock", String(filter.stockId));
  if (filter.impact) params.set("impact", filter.impact);
  if (limit && limit > TIMELINE_PAGE_SIZE) params.set("limit", String(limit));
  const query = params.toString();
  return query ? `/?${query}` : "/";
}

/** 종목 하나, 영향 하나를 고르는 칩(2026-10-21 결정). 링크라서 뒤로 가기·새로 고침에도 유지된다. */
export function TimelineFilters({
  stocks,
  filter,
}: {
  stocks: { id: number; name: string }[];
  filter: TimelineFilter;
}) {
  return (
    <div className="mt-4 space-y-2" data-testid="timeline-filters">
      <ChipRow label="종목">
        <Chip
          href={timelineHref({ ...filter, stockId: null })}
          active={filter.stockId === null}
        >
          전체
        </Chip>
        {stocks.map((stock) => (
          <Chip
            key={stock.id}
            href={timelineHref({ ...filter, stockId: stock.id })}
            active={filter.stockId === stock.id}
          >
            {stock.name}
          </Chip>
        ))}
      </ChipRow>
      <ChipRow label="영향">
        <Chip
          href={timelineHref({ ...filter, impact: null })}
          active={filter.impact === null}
        >
          전체
        </Chip>
        {IMPACTS.map((impact) => (
          <Chip
            key={impact}
            href={timelineHref({ ...filter, impact })}
            active={filter.impact === impact}
          >
            {IMPACT_LABEL[impact]}
          </Chip>
        ))}
      </ChipRow>
    </div>
  );
}

function ChipRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <nav aria-label={`${label} 필터`} className="flex items-start gap-3">
      <span className="w-8 shrink-0 pt-1 text-xs text-muted-foreground">
        {label}
      </span>
      <div className="flex flex-wrap gap-1.5">{children}</div>
    </nav>
  );
}

function Chip({
  href,
  active,
  children,
}: {
  href: string;
  active: boolean;
  children: ReactNode;
}) {
  return (
    <Link
      href={href}
      aria-current={active ? "true" : undefined}
      className={cn(
        "rounded-full border px-3 py-1 text-xs transition-colors",
        active
          ? "border-primary bg-primary text-primary-foreground"
          : "text-muted-foreground hover:bg-muted hover:text-foreground",
      )}
    >
      {children}
    </Link>
  );
}
