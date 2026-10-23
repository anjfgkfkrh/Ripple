import { cn } from "cn";
import { ChevronRightIcon, LoaderCircleIcon } from "lucide-react";
import { badgeVariants } from "@/components/ui/badge";
import type { KeywordStatus } from "@/server/db/schema";
import type { Keyword } from "@/server/keywords/read";
import type { Stock } from "@/server/stocks/repository";
import { formatSeoulDateTime } from "@/server/time";
import { RegenerateKeywordsButton } from "./regenerate-keywords-button";

export function isMakingKeywords(status: KeywordStatus): boolean {
  return status === "pending" || status === "generating";
}

/** 종목 한 줄 아래의 키워드 영역: 생성 상태, 펼쳐 보는 트리, 다시 만들기 (SPEC FR-3) */
export function StockKeywords({
  stock,
  keywords,
}: {
  stock: Stock;
  keywords: Keyword[];
}) {
  const name = stock.nameKo ?? stock.name;
  const depth = Math.max(0, ...keywords.map((keyword) => keyword.level));

  return (
    <div className="mt-2 space-y-2" data-testid="stock-keywords">
      {isMakingKeywords(stock.keywordsStatus) && (
        <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <LoaderCircleIcon className="size-3.5 animate-spin" aria-hidden />
          키워드를 만드는 중…
        </p>
      )}
      {stock.keywordsStatus === "failed" && (
        <div className="space-y-1.5">
          <p className="text-xs text-destructive">
            {keywords.length > 0
              ? "새 키워드를 만들지 못해 이전 키워드를 그대로 둡니다."
              : "키워드를 만들지 못했습니다."}{" "}
            {stock.keywordsError}
          </p>
          <RegenerateKeywordsButton id={stock.id} name={name} />
        </div>
      )}
      {keywords.length > 0 && (
        <details className="group">
          <summary className="flex w-fit cursor-pointer list-none items-center gap-1 text-xs text-muted-foreground select-none [&::-webkit-details-marker]:hidden">
            <ChevronRightIcon
              className="size-3.5 transition-transform group-open:rotate-90"
              aria-hidden
            />
            <span className="font-medium text-foreground">
              키워드 {keywords.length}개
            </span>
            <span>· 깊이 {depth}</span>
            {stock.keywordsGeneratedAt && (
              <span>
                · {formatSeoulDateTime(stock.keywordsGeneratedAt).slice(0, 16)}
              </span>
            )}
          </summary>
          <div className="mt-2 rounded-md bg-muted/50 p-3">
            {/* 종목 이름을 맨 위에 두어 들여쓰기만으로 경로가 읽히게 한다. */}
            <p className="text-sm font-semibold">{name}</p>
            <div className="mt-2 ml-1 border-l pl-3">
              <KeywordTree nodes={toTree(keywords)} />
            </div>
            {stock.keywordsStatus === "ready" && (
              <RegenerateKeywordsButton
                id={stock.id}
                name={name}
                className="mt-3"
              />
            )}
          </div>
        </details>
      )}
    </div>
  );
}

type KeywordNode = Keyword & { children: KeywordNode[] };

/** 단계·생성 순서로 받은 키워드를 부모 아래로 모은다. 부모가 없으면 맨 위에 둔다. */
function toTree(keywords: Keyword[]): KeywordNode[] {
  const nodes = new Map<number, KeywordNode>(
    keywords.map((keyword) => [keyword.id, { ...keyword, children: [] }]),
  );
  const roots: KeywordNode[] = [];
  for (const node of nodes.values()) {
    const parent =
      node.parentId === null ? undefined : nodes.get(node.parentId);
    (parent?.children ?? roots).push(node);
  }
  return roots;
}

/**
 * 들여쓰기가 곧 경로다(예: 삼성전자 → 메모리 → 엔비디아). 마우스를 올리면 경로 글자도 보인다.
 * 시사 가지는 맨 위 키워드에 '시사'를 붙인다(아래 키워드는 같은 가지라 붙이지 않음, 10단계).
 * 토픽 갱신이 붙인 가지는 '토픽'을 붙이고, Jev가 매긴 관련도·영향도를 보인다(11단계).
 */
function KeywordTree({ nodes }: { nodes: KeywordNode[] }) {
  return (
    <ul className="space-y-2">
      {nodes.map((node) => (
        <li key={node.id} data-testid="keyword">
          <p className="text-sm" title={node.path}>
            <span className="font-medium">{node.textKo}</span>
            {node.textEn && (
              <span className="ml-1.5 text-xs text-muted-foreground">
                {node.textEn}
              </span>
            )}
            {node.parentId === null && node.topicId !== null ? (
              <span
                className={cn(
                  badgeVariants({ variant: "secondary" }),
                  "ml-1.5 align-text-bottom",
                )}
                data-testid="topic-badge"
              >
                토픽
              </span>
            ) : (
              node.kind === "current" &&
              node.parentId === null && (
                <span
                  className={cn(
                    badgeVariants({ variant: "outline" }),
                    "ml-1.5 align-text-bottom",
                  )}
                  data-testid="current-badge"
                >
                  시사
                </span>
              )
            )}
            {node.relevance !== null && node.magnitude !== null && (
              <span
                className="ml-1.5 text-xs whitespace-nowrap text-muted-foreground tabular-nums"
                data-testid="keyword-scores"
              >
                관련 {node.relevance.toFixed(1)} · 영향{" "}
                {node.magnitude.toFixed(1)}
              </span>
            )}
          </p>
          {node.reason && (
            <p className="text-xs text-muted-foreground">{node.reason}</p>
          )}
          {node.children.length > 0 && (
            <div className="mt-2 ml-1 border-l pl-3">
              <KeywordTree nodes={node.children} />
            </div>
          )}
        </li>
      ))}
    </ul>
  );
}
