import { Suspense } from "react";
import { AutoRefresh } from "@/components/auto-refresh";
import { requireSession } from "@/server/auth/guard";
import { getWebDb } from "@/server/db/web";
import { listKeywordsByStock } from "@/server/keywords/read";
import { listActiveStocks } from "@/server/stocks/repository";
import { DeleteStockButton } from "./delete-stock-button";
import { MarketTopicSection } from "./market-topics";
import { isMakingKeywords, StockKeywords } from "./stock-keywords";
import { StockMeta } from "./stock-meta";
import { StockSearch } from "./stock-search";

export default function StocksPage() {
  return (
    <main className="mx-auto w-full max-w-xl px-4 py-8 sm:py-12">
      <h1 className="text-xl font-semibold">종목</h1>
      <p className="mt-1 text-sm text-muted-foreground">
        보유 종목을 이름이나 티커로 찾아 등록합니다. 등록하면 뉴스 검색에 쓸
        연관 키워드를 AI가 만듭니다.
      </p>
      <StockSearch />
      <section className="mt-10">
        <h2 className="text-base font-semibold">등록한 종목</h2>
        <Suspense
          fallback={
            <p className="mt-3 text-sm text-muted-foreground">불러오는 중…</p>
          }
        >
          <StockList />
        </Suspense>
      </section>
      <section className="mt-10" data-testid="market-topics">
        <h2 className="text-base font-semibold">시장·주제</h2>
        <p className="mt-1 text-sm text-muted-foreground">
          종목과 별개로 증시 전체와 직접 넣은 주제에 영향을 줄 기사를 모읍니다.
          시장을 움직이는 토픽은 AI가 정해진 때마다 따로 잡아 타임라인 맨 위에
          보입니다.
        </p>
        <Suspense
          fallback={
            <p className="mt-3 text-sm text-muted-foreground">불러오는 중…</p>
          }
        >
          <MarketTopicSection />
        </Suspense>
      </section>
    </main>
  );
}

async function StockList() {
  // 데이터에 닿기 전에 로그인을 한 번 더 확인한다(요청 시점 실행).
  await requireSession();
  const db = getWebDb();
  const stocks = listActiveStocks(db);
  if (stocks.length === 0) {
    return (
      <p className="mt-3 text-sm text-muted-foreground">
        아직 등록한 종목이 없습니다.
      </p>
    );
  }
  const keywordsByStock = listKeywordsByStock(
    db,
    stocks.map((stock) => stock.id),
  );
  return (
    <>
      <AutoRefresh
        active={stocks.some((stock) => isMakingKeywords(stock.keywordsStatus))}
      />
      <ul className="mt-3 divide-y rounded-lg border" data-testid="stock-list">
        {stocks.map((stock) => {
          const displayName = stock.nameKo ?? stock.name;
          return (
            <li key={stock.id} className="p-3" data-testid="stock">
              <div className="flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <p className="truncate font-medium">{displayName}</p>
                  <StockMeta {...stock} />
                </div>
                <DeleteStockButton id={stock.id} name={displayName} />
              </div>
              <StockKeywords
                stock={stock}
                keywords={keywordsByStock.get(stock.id) ?? []}
              />
            </li>
          );
        })}
      </ul>
    </>
  );
}
