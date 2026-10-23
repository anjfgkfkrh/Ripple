import { AutoRefresh } from "@/components/auto-refresh";
import { requireSession } from "@/server/auth/guard";
import { getWebDb } from "@/server/db/web";
import { listKeywordsByStock } from "@/server/keywords/read";
import { listMarketItems, listUserTopicItems } from "@/server/topics/read";
import { DeleteTopicButton } from "./delete-topic-button";
import { MarketToggle } from "./market-toggle";
import { isMakingKeywords, StockKeywords } from "./stock-keywords";
import { TopicForm } from "./topic-form";

/**
 * 종목 화면의 '시장·주제' 구역(11단계 결정): 한국·미국 증시 켜기/끄기, 주제 넣기·지우기.
 * 키워드 트리 보기·다시 만들기는 종목과 같다. 주제는 켜진 증시마다 따로 평가한다.
 */
export async function MarketTopicSection() {
  // 데이터에 닿기 전에 로그인을 한 번 더 확인한다(요청 시점 실행).
  await requireSession();
  const db = getWebDb();
  const markets = listMarketItems(db);
  const userTopics = listUserTopicItems(db);
  const items = [
    ...markets.flatMap((m) => (m.stock ? [m.stock] : [])),
    ...userTopics.flatMap((t) => t.lenses),
  ];
  const keywordsByStock = listKeywordsByStock(
    db,
    items.map((item) => item.id),
  );
  const enabledCount = markets.filter((m) => m.stock).length;

  return (
    <>
      <AutoRefresh
        active={items.some((item) => isMakingKeywords(item.keywordsStatus))}
      />
      <ul className="mt-3 divide-y rounded-lg border" data-testid="market-list">
        {markets.map((m) => (
          <li key={m.market} className="p-3" data-testid="market">
            <div className="flex items-center justify-between gap-3">
              <div className="min-w-0">
                <p className="font-medium">{m.name}</p>
                <p className="mt-1 text-xs text-muted-foreground">
                  {m.stock
                    ? `${m.stock.exchange} · 시장 전체에 영향을 주는 기사를 모읍니다`
                    : "꺼짐"}
                </p>
              </div>
              <MarketToggle
                market={m.market}
                name={m.name}
                enabled={m.stock !== null}
              />
            </div>
            {m.stock && (
              <StockKeywords
                stock={m.stock}
                keywords={keywordsByStock.get(m.stock.id) ?? []}
              />
            )}
          </li>
        ))}
      </ul>

      <div className="mt-6">
        <h3 className="text-sm font-semibold">주제</h3>
        <p className="mt-1 text-sm text-muted-foreground">
          부동산 시장, 중동 정세처럼 따로 보고 싶은 주제를 넣으면 켜진 증시
          기준으로 키워드 트리를 만들어 기사를 모읍니다.
        </p>
        <div className="mt-3">
          <TopicForm />
        </div>
        {userTopics.length > 0 && (
          <ul
            className="mt-3 divide-y rounded-lg border"
            data-testid="topic-list"
          >
            {userTopics.map(({ topic, lenses }) => (
              <li key={topic.id} className="p-3" data-testid="user-topic">
                <div className="flex items-center justify-between gap-3">
                  <p className="min-w-0 truncate font-medium">{topic.name}</p>
                  <DeleteTopicButton id={topic.id} name={topic.name} />
                </div>
                {lenses.length === 0 ? (
                  <p className="mt-1 text-xs text-muted-foreground">
                    켜진 증시가 없어 이 주제로는 기사를 모으지 않습니다.
                  </p>
                ) : (
                  lenses.map((lens) => (
                    <div key={lens.id}>
                      {enabledCount > 1 && (
                        <p className="mt-2 text-xs font-medium text-muted-foreground">
                          {lens.exchange} 기준
                        </p>
                      )}
                      <StockKeywords
                        stock={lens}
                        keywords={keywordsByStock.get(lens.id) ?? []}
                      />
                    </div>
                  ))
                )}
              </li>
            ))}
          </ul>
        )}
      </div>
    </>
  );
}
