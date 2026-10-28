"use client";

import {
  startTransition,
  useOptimistic,
  useState,
  type MouseEvent,
  type ReactNode,
} from "react";
import { cn } from "cn";
import { setArticlesReadAction } from "./actions";

/**
 * 기사 카드의 겉틀(12단계 읽음 표시). 카드 안 원문 링크를 누르면(가운데 버튼 포함) 카드에 묶인 기사를
 * 모두 읽음으로 저장하고, 아래 작은 버튼으로 읽음/안 읽음을 되돌린다.
 * 저장하는 동안에도 화면은 바로 바뀌고, 실패하면 원래대로 돌아가며 이유를 보인다.
 * 읽은 카드에서 안쪽 글자를 흐리게 하는 것은 안쪽 마크업이 `group-data-[read=true]:`로 정한다.
 */
export function ReadCard({
  articleIds,
  title,
  read,
  children,
}: {
  /** 카드에 묶인 기사(대표 포함) */
  articleIds: number[];
  title: string;
  read: boolean;
  children: ReactNode;
}) {
  const [isRead, setRead] = useOptimistic(read);
  const [error, setError] = useState<string | null>(null);

  const save = (next: boolean) => {
    setError(null);
    startTransition(async () => {
      setRead(next);
      const result = await setArticlesReadAction(articleIds, next);
      if (result.error) setError(result.error);
    });
  };
  /** 카드 안 링크(제목, "외 N곳"의 언론사)를 눌렀을 때만 읽음으로 저장한다. */
  const onLinkOpen = (event: MouseEvent<HTMLElement>) => {
    if (isRead) return;
    if (event.target instanceof Element && event.target.closest("a[href]")) {
      save(true);
    }
  };

  return (
    <article
      className={cn("group rounded-lg border p-4", isRead && "bg-muted/40")}
      data-testid="timeline-card"
      data-read={isRead}
      onClick={onLinkOpen}
      onAuxClick={(event) => {
        if (event.button === 1) onLinkOpen(event);
      }}
    >
      {children}
      <div className="mt-3 flex items-center justify-end gap-2 text-xs text-muted-foreground">
        {error && (
          <span role="alert" className="mr-auto text-destructive">
            {error}
          </span>
        )}
        {isRead && <span data-testid="read-label">읽음 ·</span>}
        <button
          type="button"
          onClick={() => save(!isRead)}
          aria-label={`${title} — ${isRead ? "안 읽음으로" : "읽음으로 표시"}`}
          className="underline-offset-2 hover:underline"
          data-testid="read-toggle"
        >
          {isRead ? "안 읽음으로" : "읽음으로 표시"}
        </button>
      </div>
    </article>
  );
}
