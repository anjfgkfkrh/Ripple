import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Ripple",
  description: "보유 종목 관련 뉴스·증권사 리포트 모아보기",
  // 공개 URL(Cloudflare Tunnel)로 노출되므로 검색엔진 수집을 막는다.
  robots: { index: false, follow: false },
};

export default function RootLayout({ children }: LayoutProps<"/">) {
  return (
    <html lang="ko" className="h-full font-sans antialiased">
      {/* 한글은 낱말 중간에서 줄을 바꾸지 않고, 너무 긴 낱말만 넘치지 않게 자른다. */}
      <body className="flex min-h-full flex-col break-keep wrap-break-word">
        {children}
      </body>
    </html>
  );
}
