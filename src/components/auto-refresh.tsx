"use client";

import { useRouter } from "next/navigation";
import { useEffect } from "react";

/** active인 동안 화면을 주기적으로 다시 읽는다(키워드 생성·기사 수집 상태 표시). */
export function AutoRefresh({
  active,
  intervalMs = 3000,
}: {
  active: boolean;
  intervalMs?: number;
}) {
  const router = useRouter();
  useEffect(() => {
    if (!active) return;
    const timer = setInterval(() => router.refresh(), intervalMs);
    return () => clearInterval(timer);
  }, [active, intervalMs, router]);
  return null;
}
