import Link from "next/link";
import { cn } from "cn";
import { ChevronRightIcon } from "lucide-react";
import type { TimelineFilter, TopicCard } from "@/server/timeline/read";
import { formatTimelineTime } from "@/server/time";
import { ImpactBadge } from "./impact";
import { timelineHref } from "./timeline-filters";

/**
 * 타임라인 맨 위 '지금 시장 이슈'(11단계 결정): AI가 정해진 때마다 잡는 자동 토픽.
 * 카드를 누르면 그 토픽 기사만 보이고, 한 번 더 누르면 풀린다. 12시간 넘게 새 기사가 없으면 '지난 이슈'로 접는다.
 */
export function TopicCards({
  cards,
  filter,
  now,
}: {
  cards: TopicCard[];
  filter: TimelineFilter;
  now: Date;
}) {
  if (cards.length === 0) return null;
  const active = cards.filter((card) => !card.faded);
  const faded = cards.filter((card) => card.faded);
  return (
    <section
      className="mt-4"
      aria-label="지금 시장 이슈"
      data-testid="topic-cards"
    >
      <h2 className="text-sm font-medium">지금 시장 이슈</h2>
      {active.length > 0 ? (
        <CardStrip cards={active} filter={filter} now={now} />
      ) : (
        <p className="mt-1 text-xs text-muted-foreground">
          지난 12시간 동안 새 기사가 걸린 토픽이 없습니다.
        </p>
      )}
      {faded.length > 0 && (
        <details
          className="group mt-1"
          open={faded.some((card) => card.topicId === filter.topicId)}
        >
          <summary className="flex w-fit cursor-pointer list-none items-center gap-1 text-xs text-muted-foreground select-none [&::-webkit-details-marker]:hidden">
            <ChevronRightIcon
              className="size-3.5 transition-transform group-open:rotate-90"
              aria-hidden
            />
            지난 이슈 {faded.length}개
          </summary>
          <CardStrip cards={faded} filter={filter} now={now} />
        </details>
      )}
    </section>
  );
}

/** 좁은 화면에서는 옆으로 밀어 본다(페이지는 가로로 넘치지 않음). */
function CardStrip({
  cards,
  filter,
  now,
}: {
  cards: TopicCard[];
  filter: TimelineFilter;
  now: Date;
}) {
  return (
    <ul className="-mx-4 mt-2 flex snap-x gap-2 overflow-x-auto px-4 pb-2">
      {cards.map((card) => {
        const selected = filter.topicId === card.topicId;
        return (
          <li key={card.topicId} className="w-64 shrink-0 snap-start">
            <Link
              href={timelineHref({
                ...filter,
                stockId: null,
                topicId: selected ? null : card.topicId,
              })}
              scroll={false}
              aria-current={selected ? "true" : undefined}
              className={cn(
                "flex h-full flex-col gap-1.5 rounded-lg border p-3 transition-colors",
                selected ? "border-primary bg-primary/5" : "hover:bg-muted/60",
                card.faded && !selected && "opacity-75",
              )}
              data-testid="topic-card"
            >
              <span className="flex items-start justify-between gap-2">
                <span className="leading-snug font-medium">{card.name}</span>
                <span className="flex shrink-0 gap-1">
                  {card.impacts.map((item) => (
                    <span key={item.market} className="flex items-center gap-1">
                      {card.impacts.length > 1 && (
                        <span className="text-xs text-muted-foreground">
                          {item.market === "KR" ? "한국" : "미국"}
                        </span>
                      )}
                      <ImpactBadge impact={item.impact} />
                    </span>
                  ))}
                </span>
              </span>
              {card.summary && (
                <span className="line-clamp-2 text-xs text-muted-foreground">
                  {card.summary}
                </span>
              )}
              <span className="text-xs text-muted-foreground tabular-nums">
                기사 {card.shownTotal}건
                {card.shown24h !== card.shownTotal &&
                  ` (24시간 ${card.shown24h}건)`}{" "}
                · 마지막 {formatTimelineTime(card.lastArticleAt, now)}
              </span>
              {card.related.length > 0 && (
                <span
                  className="flex flex-wrap items-center gap-1 text-xs"
                  data-testid="topic-related"
                >
                  <span className="text-muted-foreground">관련 내 종목</span>
                  {card.related.map((stock) => (
                    <span
                      key={stock.stockId}
                      className="flex items-center gap-1"
                    >
                      <span>{stock.name}</span>
                      {stock.impact && <ImpactBadge impact={stock.impact} />}
                    </span>
                  ))}
                </span>
              )}
            </Link>
          </li>
        );
      })}
    </ul>
  );
}
