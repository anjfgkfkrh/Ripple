import { count } from "drizzle-orm";
import { readJournalMode, type Db } from "@/server/db/client";
import { settings } from "@/server/db/schema";
import { fetchWorkerHealth, type WorkerHealth } from "@/server/worker-client";

export type DatabaseStatus =
  | { ok: true; journalMode: string; settingsCount: number }
  | { ok: false; reason: string };

export type SystemStatus = {
  database: DatabaseStatus;
  worker: WorkerHealth;
  checkedAt: Date;
};

/** 0단계 확인 화면용: web에서 DB(읽기 전용)와 worker 내부 API에 닿는지 확인한다. */
export async function getSystemStatus(options: {
  getDb: () => Db;
  workerUrl: string | undefined;
}): Promise<SystemStatus> {
  const database = checkDatabase(options.getDb);
  const worker = options.workerUrl
    ? await fetchWorkerHealth(options.workerUrl)
    : {
        ok: false as const,
        reason: "환경변수 WORKER_URL이 설정되지 않았습니다.",
      };
  return { database, worker, checkedAt: new Date() };
}

function checkDatabase(getDb: () => Db): DatabaseStatus {
  try {
    const db = getDb();
    const row = db.select({ n: count() }).from(settings).get();
    return {
      ok: true,
      journalMode: readJournalMode(db),
      settingsCount: row?.n ?? 0,
    };
  } catch (error) {
    return {
      ok: false,
      reason: error instanceof Error ? error.message : String(error),
    };
  }
}
