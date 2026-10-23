import { Fragment } from "react";
import type { TimelineCard, TimelineStock } from "@/server/timeline/read";
import { formatTimelineTime } from "@/server/time";
import { ImpactBadge } from "./impact";

const KIND_LABEL = { market: "시장", topic: "주제" } as const;

/**
 * 기사 카드(SPEC FR-7): 제목, 출처, 시각, 관련 항목(점수·호재/악재·걸린 경위), 원문 링크(새 탭).
 * 11단계: 시장·주제 항목은 이름 앞에 작게 '시장'·'주제'를 붙이고 영향도도 보인다.
 * 같은 주제를 두 증시 기준으로 평가했으면 어느 증시 기준인지 붙인다.
 */
export function TimelineCardView({
  card,
  now,
}: {
  card: TimelineCard;
  now: Date;
}) {
  const topicCounts = new Map<number, number>();
  for (const stock of card.stocks) {
    if (stock.topicId !== null) {
      topicCounts.set(stock.topicId, (topicCounts.get(stock.topicId) ?? 0) + 1);
    }
  }
  return (
    <article className="rounded-lg border p-4" data-testid="timeline-card">
      <h3 className="leading-snug font-medium">
        <a
          href={card.url}
          target="_blank"
          rel="noopener noreferrer"
          className="hover:underline"
        >
          {card.title}
          <span className="sr-only"> (새 탭에서 열림)</span>
        </a>
      </h3>
      <p className="mt-1 text-xs text-muted-foreground">
        {card.publisher} · {formatTimelineTime(card.publishedAt, now)}
      </p>
      <ul className="mt-3 space-y-2" aria-label="관련 항목">
        {card.stocks.map((stock) => (
          <li
            key={stock.stockId}
            className="text-sm"
            data-testid="timeline-stock"
          >
            <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
              <ImpactBadge impact={stock.impact} />
              <span className="font-medium">
                {stock.kind !== "stock" && (
                  <span className="mr-1 text-xs font-normal text-muted-foreground">
                    {KIND_LABEL[stock.kind]}
                  </span>
                )}
                {stock.name}
                {stock.topicId !== null &&
                  (topicCounts.get(stock.topicId) ?? 0) > 1 && (
                    <span className="ml-1 text-xs font-normal text-muted-foreground">
                      ({stock.market === "KR" ? "한국" : "미국"} 증시 기준)
                    </span>
                  )}
              </span>
              <span className="text-muted-foreground tabular-nums">
                관련도 {stock.relevance.toFixed(1)}
                {stock.magnitude !== null &&
                  ` · 영향도 ${stock.magnitude.toFixed(1)}`}
              </span>
            </div>
            <p className="mt-0.5 text-xs text-muted-foreground">
              {describeLink(stock)}
            </p>
          </li>
        ))}
      </ul>
      {card.others.length > 0 && (
        <p className="mt-3 text-xs text-muted-foreground">
          외 {card.others.length}곳:{" "}
          {card.others.map((other, index) => (
            <Fragment key={other.articleId}>
              {index > 0 && " · "}
              <a
                href={other.url}
                target="_blank"
                rel="noopener noreferrer"
                className="underline-offset-2 hover:underline"
              >
                {other.publisher}
              </a>
            </Fragment>
          ))}
        </p>
      )}
    </article>
  );
}

/** 이 기사가 이 항목에 걸린 경위(왜 이 기사가 나왔는지) */
function describeLink(stock: TimelineStock): string {
  if (stock.matchKind === "direct") return "Finnhub가 이 종목 뉴스로 분류";
  if (stock.matchKind === "name") return "기사에 종목 이름이 나옴";
  return `키워드: ${stock.path}`;
}
