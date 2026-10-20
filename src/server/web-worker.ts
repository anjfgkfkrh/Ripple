import { readWorkerApiToken } from "@/server/auth/worker-token";
import { requireEnv } from "@/server/env";
import { createWorkerApi } from "@/server/worker-client";

/** web의 서버 액션에서 worker 내부 API를 부른다(쓰기·AI 호출은 worker가 맡는다, D-36). */
export function webWorkerApi() {
  return createWorkerApi({
    workerUrl: requireEnv("WORKER_URL"),
    token: readWorkerApiToken(),
  });
}
