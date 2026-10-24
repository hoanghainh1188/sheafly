import type { LoadCell } from "../../types";

/**
 * How full a cell is, as the heatmap colours it: no working time, nothing to do, some
 * work, nearly full (from 80%), or more work than capacity. Work with no capacity at all
 * (a day off with work on it) is over.
 */
export type LoadTone = "off" | "free" | "light" | "full" | "over";

/** From this share of capacity a cell counts as nearly full. */
export const FULL_FROM = 0.8;

export function loadTone(cell: LoadCell): LoadTone {
  const demand = cell.work + (cell.unassigned ?? 0);
  if (cell.capacity <= 0) return demand > 0 ? "over" : "off";
  if (demand <= 0) return "free";
  const share = demand / cell.capacity;
  if (share > 1) return "over";
  return share >= FULL_FROM ? "full" : "light";
}
