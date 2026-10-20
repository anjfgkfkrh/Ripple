import { describe, expect, it } from "vitest";
import { isAuthorizedBearer, readWorkerApiToken } from "./worker-token";

const TOKEN = "t".repeat(32);

describe("readWorkerApiToken", () => {
  it("32자 이상이면 그대로 쓴다", () => {
    expect(readWorkerApiToken({ WORKER_API_TOKEN: TOKEN })).toBe(TOKEN);
  });

  it("없거나 짧으면 실패하고, 값은 메시지에 넣지 않는다", () => {
    expect(() => readWorkerApiToken({})).toThrow("WORKER_API_TOKEN");
    expect(() =>
      readWorkerApiToken({ WORKER_API_TOKEN: "short-token" }),
    ).toThrow(expect.not.stringContaining("short-token") as unknown as string);
  });
});

describe("isAuthorizedBearer", () => {
  it("Bearer 토큰이 같을 때만 참", () => {
    expect(isAuthorizedBearer(`Bearer ${TOKEN}`, TOKEN)).toBe(true);
    expect(isAuthorizedBearer(`Bearer ${TOKEN}x`, TOKEN)).toBe(false);
    expect(isAuthorizedBearer(TOKEN, TOKEN)).toBe(false);
    expect(isAuthorizedBearer("Bearer ", TOKEN)).toBe(false);
    expect(isAuthorizedBearer(undefined, TOKEN)).toBe(false);
  });
});
