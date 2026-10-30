import { describe, expect, it } from "vitest";
import { createKisClient, KisError, parseQuote, TOKEN_RETRY_MS } from "./kis";

type Call = { url: string; init: RequestInit };

/** 가짜 KIS 서버. 토큰 요청과 시세 요청에 차례로 답한다. */
function fakeKis(respond: (call: Call) => { status: number; body: unknown }): {
  fetchImpl: typeof fetch;
  calls: Call[];
} {
  const calls: Call[] = [];
  const fetchImpl = (async (url: string, init: RequestInit) => {
    const call = { url: String(url), init };
    calls.push(call);
    const { status, body } = respond(call);
    return new Response(JSON.stringify(body), { status });
  }) as typeof fetch;
  return { fetchImpl, calls };
}

const TOKEN_BODY = {
  access_token: "tok-1",
  access_token_token_expired: "2026-10-31 10:00:00",
  token_type: "Bearer",
  expires_in: 86400,
};
const PRICE_BODY = {
  rt_cd: "0",
  msg_cd: "MCA00000",
  msg1: "정상처리 되었습니다.",
  output: { stck_prpr: "152300", stck_sdpr: "150000", prdy_vrss: "2300" },
};

function client(
  fake: { fetchImpl: typeof fetch },
  clock: { at: number } = { at: Date.parse("2026-10-30T01:00:00Z") },
) {
  return createKisClient({
    appKey: "app-key",
    appSecret: "app-secret",
    env: "demo",
    fetchImpl: fake.fetchImpl,
    now: () => new Date(clock.at),
    minIntervalMs: 0,
  });
}

describe("createKisClient (13단계)", () => {
  it("토큰을 한 번 받아 두고, 통합 시장(UN) 현재가를 공식 예제의 헤더로 묻는다", async () => {
    const fake = fakeKis((call) =>
      call.url.endsWith("/oauth2/tokenP")
        ? { status: 200, body: TOKEN_BODY }
        : { status: 200, body: PRICE_BODY },
    );
    const kis = client(fake);
    expect(await kis.domesticQuote("000720")).toEqual({
      price: 152300,
      change: 2300,
      changePercent: 1.53,
    });
    await kis.domesticQuote("034020");
    expect(fake.calls.map((c) => c.url)).toEqual([
      "https://openapivts.koreainvestment.com:29443/oauth2/tokenP",
      "https://openapivts.koreainvestment.com:29443/uapi/domestic-stock/v1/quotations/inquire-price?FID_COND_MRKT_DIV_CODE=UN&FID_INPUT_ISCD=000720",
      "https://openapivts.koreainvestment.com:29443/uapi/domestic-stock/v1/quotations/inquire-price?FID_COND_MRKT_DIV_CODE=UN&FID_INPUT_ISCD=034020",
    ]);
    expect(fake.calls[0].init.method).toBe("POST");
    expect(JSON.parse(String(fake.calls[0].init.body))).toEqual({
      grant_type: "client_credentials",
      appkey: "app-key",
      appsecret: "app-secret",
    });
    expect(fake.calls[1].init.headers).toEqual({
      "content-type": "application/json; charset=utf-8",
      authorization: "Bearer tok-1",
      appkey: "app-key",
      appsecret: "app-secret",
      tr_id: "FHKST01010100",
      custtype: "P",
    });
  });

  it("만료가 다가오면 새 토큰을 받는다", async () => {
    let issued = 0;
    const fake = fakeKis((call) =>
      call.url.endsWith("/oauth2/tokenP")
        ? {
            status: 200,
            body: { ...TOKEN_BODY, access_token: `tok-${++issued}` },
          }
        : { status: 200, body: PRICE_BODY },
    );
    const clock = { at: Date.parse("2026-10-30T01:00:00Z") };
    const kis = client(fake, clock);
    await kis.domesticQuote("000720");
    clock.at += 23 * 60 * 60 * 1000;
    await kis.domesticQuote("000720");
    expect(issued).toBe(1);
    clock.at += 55 * 60 * 1000;
    await kis.domesticQuote("000720");
    expect(issued).toBe(2);
    expect(fake.calls.at(-1)!.init.headers).toMatchObject({
      authorization: "Bearer tok-2",
    });
  });

  it("토큰 문제(EGW001xx)면 새 토큰으로 한 번만 다시 묻는다", async () => {
    let issued = 0;
    let priceCalls = 0;
    const fake = fakeKis((call) => {
      if (call.url.endsWith("/oauth2/tokenP")) {
        return {
          status: 200,
          body: { ...TOKEN_BODY, access_token: `tok-${++issued}` },
        };
      }
      priceCalls++;
      return priceCalls === 1
        ? {
            status: 500,
            body: {
              rt_cd: "1",
              msg_cd: "EGW00123",
              msg1: "기간이 만료된 token 입니다.",
            },
          }
        : { status: 200, body: PRICE_BODY };
    });
    const clock = { at: Date.parse("2026-10-30T01:00:00Z") };
    const kis = client(fake, clock);
    await expect(kis.domesticQuote("000720")).resolves.toMatchObject({
      price: 152300,
    });
    expect(issued).toBe(2);
  });

  it("시세 오류는 HTTP 상태·코드·메시지를 담아 던진다", async () => {
    const fake = fakeKis((call) =>
      call.url.endsWith("/oauth2/tokenP")
        ? { status: 200, body: TOKEN_BODY }
        : {
            status: 200,
            body: { rt_cd: "1", msg_cd: "OPSQ0002", msg1: "없는 종목 " },
          },
    );
    const call = client(fake).domesticQuote("999999");
    await expect(call).rejects.toBeInstanceOf(KisError);
    await expect(call).rejects.toThrow(
      "KIS 시세 오류(HTTP 200, OPSQ0002): 없는 종목",
    );
  });

  it("토큰 발급이 실패하면 1분(발급 제한) 동안은 다시 요청하지 않는다", async () => {
    let tokenCalls = 0;
    const fake = fakeKis((call) => {
      if (call.url.endsWith("/oauth2/tokenP")) {
        tokenCalls++;
        return tokenCalls === 1
          ? {
              status: 403,
              body: {
                error_description: "접근토큰 발급 잠시 후 다시 시도하세요.",
              },
            }
          : { status: 200, body: TOKEN_BODY };
      }
      return { status: 200, body: PRICE_BODY };
    });
    const clock = { at: Date.parse("2026-10-30T01:00:00Z") };
    const kis = client(fake, clock);
    await expect(kis.domesticQuote("000720")).rejects.toThrow(
      "KIS 토큰을 받지 못했습니다: 접근토큰 발급 잠시 후 다시 시도하세요.",
    );
    clock.at += TOKEN_RETRY_MS - 1000;
    await expect(kis.domesticQuote("000720")).rejects.toThrow("1분 뒤");
    expect(tokenCalls).toBe(1);
    clock.at += 1000;
    await expect(kis.domesticQuote("000720")).resolves.toMatchObject({
      price: 152300,
    });
    expect(tokenCalls).toBe(2);
  });

  it("실전 키는 실전 주소를 쓴다", async () => {
    const fake = fakeKis(() => ({ status: 200, body: TOKEN_BODY }));
    await createKisClient({
      appKey: "k",
      appSecret: "s",
      env: "real",
      fetchImpl: fake.fetchImpl,
      minIntervalMs: 0,
    })
      .domesticQuote("000720")
      .catch(() => undefined);
    expect(fake.calls[0].url).toBe(
      "https://openapi.koreainvestment.com:9443/oauth2/tokenP",
    );
  });
});

describe("parseQuote", () => {
  it("현재가와 기준가로 대비·등락률(소수 둘째 자리)을 계산한다", () => {
    expect(parseQuote({ stck_prpr: "98700", stck_sdpr: "100000" })).toEqual({
      price: 98700,
      change: -1300,
      changePercent: -1.3,
    });
    expect(parseQuote({ stck_prpr: "100000", stck_sdpr: "100000" })).toEqual({
      price: 100000,
      change: 0,
      changePercent: 0,
    });
  });

  it("값이 없거나 0이면 오류", () => {
    expect(() => parseQuote({ stck_prpr: "0", stck_sdpr: "100" })).toThrow(
      KisError,
    );
    expect(() => parseQuote(undefined)).toThrow("현재가·기준가");
  });
});
