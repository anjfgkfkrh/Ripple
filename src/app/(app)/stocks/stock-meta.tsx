import { badgeVariants } from "@/components/ui/badge";

/** 종목 한 줄 아래에 붙는 정보: 시장, 거래소, 코드(미국은 정식 영문 이름까지) */
export function StockMeta(props: {
  market: "KR" | "US";
  exchange: string;
  code: string;
  name: string;
  isEtf?: boolean;
}) {
  return (
    <p className="mt-1 flex flex-wrap items-center gap-x-1.5 gap-y-1 text-xs text-muted-foreground">
      <span className={badgeVariants({ variant: "secondary" })}>
        {props.market === "KR" ? "한국" : "미국"}
      </span>
      {props.isEtf && (
        <span className={badgeVariants({ variant: "outline" })}>ETF</span>
      )}
      <span>{props.exchange}</span>
      <span className="font-mono">{props.code}</span>
      {props.market === "US" && <span className="break-all">{props.name}</span>}
    </p>
  );
}
