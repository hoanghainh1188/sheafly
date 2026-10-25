import { getLang } from "../../i18n/lang";
import type { HealthCriterion, HealthLimit, HealthReason } from "../../types";

/** Criteria measured as an index (13 CPLI, 14 BEI) rather than a share. */
const INDEXES = new Set([13, 14]);

function locale(): string {
  return getLang() === "en" ? "en-US" : "vi-VN";
}

function percent(value: number): string {
  return new Intl.NumberFormat(locale(), { style: "percent", maximumFractionDigits: 1 }).format(value);
}

function index(value: number): string {
  return new Intl.NumberFormat(locale(), { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(value);
}

/** A measured value as the dialog shows it: "4,5 %" for a share, "0,97" for an index. */
export function formatHealthValue(number: number, value: number): string {
  return INDEXES.has(number) ? index(value) : percent(value);
}

/** A threshold: "≤ 5 %", "≥ 90 %", "≥ 0,95", or "0" for a criterion that allows none. */
export function formatHealthLimit(number: number, limit: HealthLimit): string {
  if (limit.kind === "at_most" && limit.value === 0) return "0";
  const sign = limit.kind === "at_most" ? "≤" : "≥";
  return `${sign} ${formatHealthValue(number, limit.value)}`;
}

/** How many tasks each reason accounts for, in the order reasons first appear. */
export function reasonCounts(criterion: HealthCriterion): [HealthReason, number][] {
  const counts = new Map<HealthReason, number>();
  for (const task of criterion.tasks) counts.set(task.reason, (counts.get(task.reason) ?? 0) + 1);
  return [...counts];
}
