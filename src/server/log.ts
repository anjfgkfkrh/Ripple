import { formatSeoulDateTime } from "./time";

export function log(message: string): void {
  console.log(`[${formatSeoulDateTime(new Date())}] ${message}`);
}

export function logError(message: string, error: unknown): void {
  console.error(`[${formatSeoulDateTime(new Date())}] ${message}`, error);
}
