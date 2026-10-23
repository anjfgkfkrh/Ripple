/** worker가 준 오류 코드를 화면에 보여 줄 문장으로 바꾼다. */
export function describeStockError(
  error: string,
  body: Record<string, unknown> = {},
): string {
  switch (error) {
    case "budget_exceeded": {
      const limit =
        typeof body.limitUsd === "number" ? ` $${body.limitUsd}` : "";
      return `이번 달 AI 예산${limit}을 모두 써서 지금은 찾을 수 없습니다.`;
    }
    case "llm_no_api_key":
      return "OpenRouter API 키가 설정되지 않았습니다. .env의 OPENROUTER_API_KEY를 확인하세요.";
    case "llm_payment_required":
      return "OpenRouter 크레딧이 부족하거나 키의 사용 한도에 걸렸습니다.";
    case "llm_rate_limited":
      return "요청이 많아 잠시 막혔습니다. 조금 뒤에 다시 시도하세요.";
    case "llm_timeout":
    case "llm_upstream":
    case "llm_invalid_output":
      return "종목을 찾지 못했습니다. 다시 시도하세요.";
    case "listings_unavailable":
      return "공식 종목 목록을 아직 받지 못했습니다. 잠시 뒤에 다시 시도하세요.";
    case "already_registered":
      return "이미 등록한 종목입니다.";
    case "not_listed":
      return "공식 종목 목록에 없는 종목이라 등록할 수 없습니다.";
    case "not_found":
      return "이미 삭제된 종목입니다.";
    case "invalid":
      return "주제는 2~20자로 적으세요.";
    case "exists":
      return "이미 넣은 주제입니다.";
    case "unavailable":
      return "worker가 토픽 갱신을 준비하지 못했습니다. worker를 다시 켜 보세요.";
    case "worker_timeout":
      return "worker 응답이 늦습니다. 다시 시도하세요.";
    case "worker_unreachable":
      return "worker에 연결할 수 없습니다. worker가 켜져 있는지 확인하세요.";
    default:
      return "처리하지 못했습니다. 다시 시도하세요.";
  }
}
