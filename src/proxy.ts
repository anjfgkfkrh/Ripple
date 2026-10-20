import { NextResponse, type NextRequest } from "next/server";
import { getAuthConfig } from "@/server/auth/config";
import { isValidSessionToken, SESSION_COOKIE } from "@/server/auth/session";

/**
 * 로그인하지 않으면 모든 페이지·API를 막는다(1차 확인, SPEC FR-1).
 * 페이지는 로그인 화면으로 보내고, API는 401로 거절한다. 예외는 로그인 화면과 정적 파일뿐이다.
 */
export async function proxy(request: NextRequest) {
  const { pathname, search } = request.nextUrl;
  if (pathname === "/login") return NextResponse.next();

  const token = request.cookies.get(SESSION_COOKIE)?.value;
  if (await isValidSessionToken(token, getAuthConfig())) {
    return NextResponse.next();
  }

  if (pathname.startsWith("/api/")) {
    return NextResponse.json({ error: "unauthorized" }, { status: 401 });
  }
  const target = `${pathname}${search}`;
  const loginUrl = new URL("/login", request.url);
  if (target !== "/") loginUrl.searchParams.set("next", target);
  // 요청과 같은 호스트로 가는 이동은 Next가 응답 직전에 상대 경로로 바꿔 준다
  // (next/dist/server/web/adapter.js). 그래서 접속한 주소(127.0.0.1, 터널 주소)가 유지된다.
  return NextResponse.redirect(loginUrl);
}

export const config = {
  matcher: ["/((?!_next/static|_next/image|favicon.ico).*)"],
};
