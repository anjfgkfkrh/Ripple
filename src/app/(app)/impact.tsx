import { cn } from "cn";
import { badgeVariants } from "@/components/ui/badge";
import type { Impact } from "@/server/db/schema";

export const IMPACT_LABEL: Record<Impact, string> = {
  positive: "호재",
  negative: "악재",
  neutral: "중립",
};

/** 한국 주식 관례대로 호재 빨강, 악재 파랑, 중립 회색(2026-10-21 결정) */
const IMPACT_CLASS: Record<Impact, string> = {
  positive: "bg-red-100 text-red-700 dark:bg-red-950/60 dark:text-red-300",
  negative: "bg-blue-100 text-blue-700 dark:bg-blue-950/60 dark:text-blue-300",
  neutral: "bg-muted text-muted-foreground",
};

export function ImpactBadge({ impact }: { impact: Impact }) {
  return (
    <span
      className={cn(
        badgeVariants({ variant: "secondary" }),
        IMPACT_CLASS[impact],
      )}
    >
      {IMPACT_LABEL[impact]}
    </span>
  );
}
