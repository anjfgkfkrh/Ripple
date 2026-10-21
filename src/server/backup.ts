import { mkdir, readdir, rm, stat } from "node:fs/promises";
import path from "node:path";
import Database from "better-sqlite3";
import type { Db } from "./db/client";
import { readSetting } from "./settings";
import {
  formatSeoulDateTime,
  lastSeoulBoundary,
  nextSeoulBoundary,
} from "./time";

/** 자동 백업 시각(한국 시간)과 남길 개수 (D-27, 2026-10-21 결정) */
export const BACKUP_HOUR = 4;
export const BACKUP_KEEP = 7;

const BACKUP_NAME = /^ripple-\d{8}-\d{6}\.db$/;

/** "ripple-20261022-040003.db"(한국 시간). 이름 순서가 시간 순서다. */
export function backupFileName(at: Date): string {
  const text = formatSeoulDateTime(at); // YYYY-MM-DD HH:mm:ss
  return `ripple-${text.slice(0, 10).replaceAll("-", "")}-${text.slice(11).replaceAll(":", "")}.db`;
}

/** 오늘 04:00(한국 시간) 뒤에 아직 백업하지 않았으면 지금. 그 시각에 꺼져 있었으면 켜질 때 한 번. */
export function backupPlan(db: Db, now: Date): { due: boolean; nextAt: Date } {
  const boundary = lastSeoulBoundary(now, BACKUP_HOUR);
  const due = readSetting(db, "backup.lastAt") < boundary.getTime();
  return { due, nextAt: due ? now : nextSeoulBoundary(now, BACKUP_HOUR) };
}

/**
 * SQLite 온라인 백업(D-27). 쓰는 중에도 일관된 사본을 만들고, WAL 없이 파일 하나로 열리게
 * 저널 모드를 DELETE로 바꿔 둔다(복원할 때 그대로 복사하면 된다). 최근 7개만 남긴다.
 */
export async function backupDatabase(
  db: Db,
  dir: string,
  now: Date = new Date(),
): Promise<{ file: string; bytes: number; removed: string[] }> {
  await mkdir(dir, { recursive: true });
  const file = backupFileName(now);
  const target = path.join(dir, file);
  await db.$client.backup(target);
  const copy = new Database(target);
  try {
    copy.pragma("journal_mode = DELETE");
  } finally {
    copy.close();
  }
  const { size } = await stat(target);

  const names = (await readdir(dir)).filter((name) => BACKUP_NAME.test(name));
  names.sort().reverse();
  const removed = names.slice(BACKUP_KEEP);
  for (const name of removed) {
    for (const suffix of ["", "-wal", "-shm", "-journal"]) {
      await rm(path.join(dir, name + suffix), { force: true });
    }
  }
  return { file, bytes: size, removed };
}
