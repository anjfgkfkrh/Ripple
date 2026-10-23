import Link from "next/link";
import { Suspense } from "react";
import { buttonVariants } from "@/components/ui/button";
import { requireSession } from "@/server/auth/guard";
import { getWebDb } from "@/server/db/web";
import { countPendingEvaluations } from "@/server/evaluate/read";
import { listActiveStocks } from "@/server/stocks/repository";
import {
  parseTimelineParams,
  readTimeline,
  readTopicCards,
  TIMELINE_PAGE_SIZE,
  type TimelineCard,
} from "@/server/timeline/read";
import { seoulDateKey, timelineDayLabel } from "@/server/time";
import {
  filterableTopicIds,
  listMarketItems,
  listUserTopicItems,
  topicName,
} from "@/server/topics/read";
import { TimelineCardView } from "./timeline-card";
import { TimelineFilters, timelineHref } from "./timeline-filters";
import { TopicCards } from "./topic-cards";

type SearchParams = Promise<Record<string, string | string[] | undefined>>;

// 통합 타임라인(SPEC FR-7, 7단계). 고른 필터와 개수는 주소에 남는다(D-25).
export default function TimelinePage({ searchParams }: PageProps<"/">) {
  return (
    <main className="mx-auto w-full max-w-xl px-4 py-6 sm:py-10">
      <h1 className="text-xl font-semibold">타임라인</h1>
      <Suspense
        fallback={
          <p className="mt-4 text-sm text-muted-foreground">불러오는 중…</p>
        }
      >
        <Timeline searchParams={searchParams as SearchParams} />
      </Suspense>
    </main>
  );
}

async function Timeline({ searchParams }: { searchParams: SearchParams }) {
  // 데이터에 닿기 전에 로그인을 한 번 더 확인한다(요청 시점 실행).
  await requireSession();
  const db = getWebDb();
  const now = new Date();
  const stocks = listActiveStocks(db);
  const markets = listMarketItems(db).flatMap((m) =>
    m.stock ? [{ id: m.stock.id, name: m.name }] : [],
  );
  const userTopics = listUserTopicItems(db).map(({ topic }) => ({
    id: topic.id,
    name: topic.name,
  }));
  const topicCards = readTopicCards(db, now);
  if (
    stocks.length === 0 &&
    markets.length === 0 &&
    userTopics.length === 0 &&
    topicCards.length === 0
  ) {
    return (
      <p className="mt-4 text-sm text-muted-foreground">
        등록한 종목이 없습니다.{" "}
        <Link href="/stocks" className="underline underline-offset-2">
          종목을 등록
        </Link>
        하면 30분마다 기사를 모아 여기에 보여 줍니다.
      </p>
    );
  }
  const { filter, limit } = parseTimelineParams(
    await searchParams,
    [...stocks.map((stock) => stock.id), ...markets.map((m) => m.id)],
    filterableTopicIds(db),
  );
  const { cards, hasMore, minRelevance, minMagnitude } = readTimeline(db, {
    filter,
    limit,
  });
  const pending = countPendingEvaluations(db);
  const filtered =
    filter.stockId !== null ||
    filter.topicId !== null ||
    filter.impact !== null;
  // 자동 토픽은 칩이 없으므로 무엇을 보고 있는지 따로 알린다.
  const autoTopic =
    filter.topicId !== null &&
    !userTopics.some((topic) => topic.id === filter.topicId)
      ? topicName(db, filter.topicId)
      : null;

  return (
    <>
      <p className="mt-1 text-sm text-muted-foreground">
        관련도 {minRelevance}점·영향도 {minMagnitude}점 이상인 기사를 최신순으로
        보여 줍니다.
      </p>
      <TopicCards cards={topicCards} filter={filter} now={now} />
      <TimelineFilters
        stocks={stocks.map((stock) => ({
          id: stock.id,
          name: stock.nameKo ?? stock.name,
        }))}
        markets={markets}
        topics={userTopics}
        filter={filter}
      />
      {autoTopic !== null && (
        <p className="mt-3 text-sm" data-testid="topic-filter">
          토픽 <strong>{autoTopic}</strong> 기사만 보는 중 ·{" "}
          <Link
            href={timelineHref({ ...filter, topicId: null })}
            scroll={false}
            className="text-muted-foreground underline underline-offset-2"
          >
            모두 보기
          </Link>
        </p>
      )}
      {pending > 0 && (
        <p className="mt-4 text-sm text-muted-foreground">
          평가를 기다리는 기사 {pending}건은 평가가 끝나면 기준에 맞는 것만
          보입니다.
        </p>
      )}
      {cards.length === 0 ? (
        <p
          className="mt-6 text-sm text-muted-foreground"
          data-testid="timeline-empty"
        >
          {filtered
            ? "조건에 맞는 기사가 없습니다."
            : `아직 관련도 ${minRelevance}점·영향도 ${minMagnitude}점 이상인 기사가 없습니다.`}
        </p>
      ) : (
        <DayGroups cards={cards} now={now} />
      )}
      {hasMore && (
        <Link
          href={timelineHref(filter, limit + TIMELINE_PAGE_SIZE)}
          scroll={false}
          className={buttonVariants({
            variant: "outline",
            className: "mt-6 w-full",
          })}
        >
          더 보기
        </Link>
      )}
    </>
  );
}

/** 날짜마다 구분선을 둔다(2026-10-21 결정). */
function DayGroups({ cards, now }: { cards: TimelineCard[]; now: Date }) {
  const days: { key: string; label: string; cards: TimelineCard[] }[] = [];
  for (const card of cards) {
    const key = seoulDateKey(card.publishedAt);
    const last = days.at(-1);
    if (last?.key === key) {
      last.cards.push(card);
    } else {
      days.push({
        key,
        label: timelineDayLabel(card.publishedAt, now),
        cards: [card],
      });
    }
  }
  return days.map((day) => (
    <section key={day.key} className="mt-6" data-testid="timeline-day">
      <h2 className="border-b pb-1 text-sm font-medium text-muted-foreground">
        {day.label}
      </h2>
      <ul className="mt-3 space-y-3">
        {day.cards.map((card) => (
          <li key={card.articleId}>
            <TimelineCardView card={card} now={now} />
          </li>
        ))}
      </ul>
    </section>
  ));
}
