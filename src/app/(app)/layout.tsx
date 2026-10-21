import Link from "next/link";
import { Suspense } from "react";
import { Button } from "@/components/ui/button";
import { logout } from "./actions";
import { BudgetBanner } from "./budget-banner";

const NAV = [
  { href: "/", label: "타임라인" },
  { href: "/stocks", label: "종목" },
  { href: "/status", label: "상태" },
  { href: "/settings", label: "설정" },
] as const;

/** 로그인한 사용자가 보는 화면의 공통 틀: 앱 이름, 메뉴, 로그아웃 버튼 */
export default function AppLayout({ children }: LayoutProps<"/">) {
  return (
    <>
      <header className="border-b">
        <div className="mx-auto flex h-12 w-full max-w-xl items-center gap-4 px-4">
          <Link href="/" className="font-semibold">
            Ripple
          </Link>
          <nav aria-label="메뉴" className="flex gap-3 text-sm">
            {NAV.map((item) => (
              <Link
                key={item.href}
                href={item.href}
                className="text-muted-foreground hover:text-foreground"
              >
                {item.label}
              </Link>
            ))}
          </nav>
          <form action={logout} className="ml-auto">
            <Button type="submit" variant="ghost" size="sm">
              로그아웃
            </Button>
          </form>
        </div>
      </header>
      <Suspense fallback={null}>
        <BudgetBanner />
      </Suspense>
      {children}
    </>
  );
}
