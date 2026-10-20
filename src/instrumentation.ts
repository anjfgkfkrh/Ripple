/** web 서버가 시작할 때 한 번 실행된다. Node 전용 코드는 따로 불러온다(Next.js 권장 방식). */
export async function register() {
  if (process.env.NEXT_RUNTIME === "nodejs") {
    const { checkStartupConfig } = await import("./instrumentation-node");
    checkStartupConfig();
  }
}
