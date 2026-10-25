import type { LoadBucket } from "../../types";

const DAY_MS = 86_400_000;

/**
 * The last day (`YYYY-MM-DD`) of `count` periods from `from`, so the load map has exactly
 * `count` columns: weeks run Monday to Sunday and months are calendar months, so a start
 * in the middle of one makes the first period partial instead of adding one at the end.
 */
export function loadSpanEnd(from: string, bucket: LoadBucket, count: number): string {
  const start = Date.parse(`${from}T00:00:00Z`);
  if (Number.isNaN(start)) return from;
  const date = new Date(start);
  if (bucket === "day") return key(start + (count - 1) * DAY_MS);
  if (bucket === "week") {
    const monday = start - ((date.getUTCDay() + 6) % 7) * DAY_MS;
    return key(monday + (count * 7 - 1) * DAY_MS);
  }
  // Day 0 of the month after the last one is that month's last day.
  return key(Date.UTC(date.getUTCFullYear(), date.getUTCMonth() + count, 0));
}

function key(ms: number): string {
  return new Date(ms).toISOString().slice(0, 10);
}
