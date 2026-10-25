import { assignmentUnitsMessages } from "./assignmentUnits.i18n";
import { formatHours, MINUTES_PER_DAY } from "./format";
import { getLang, pick } from "./i18n";

// An assignment's units are the share of each working day it takes: 1 is the whole day,
// 0.25 a quarter. Typed as hours per day ("2h/ngày", "2 hrs/day") they are taken against
// the eight-hour day durations count in, so on a calendar with shorter days 0.25 is still
// a quarter of that day. The engine only ever sees units.

/** A plain decimal: digits with at most one decimal mark, no grouping, no exponent. */
const DECIMAL = "(\\d+(?:[.,]\\d+)?|[.,]\\d+)";
const PLAIN = new RegExp(`^${DECIMAL}$`);
const PER_DAY = new RegExp(`^${DECIMAL}\\s*(?:h|giờ|g|hrs?|hours?)\\s*/\\s*(?:ngày|day|d)$`, "i");

/** More than this is a typing slip, not units (the engine itself has no limit). */
const MAX_UNITS = 1000;

function decimal(text: string): number {
  return Number(text.replace(",", "."));
}

/** The units `text` stands for, or `null` when it is not units. */
export function parseUnits(text: string): number | null {
  const trimmed = text.trim();
  const plain = PLAIN.exec(trimmed);
  const perDay = plain ? null : PER_DAY.exec(trimmed);
  const units = plain ? decimal(plain[1]) : perDay ? (decimal(perDay[1]) * 60) / MINUTES_PER_DAY : NaN;
  return Number.isFinite(units) && units >= 0 && units <= MAX_UNITS ? units : null;
}

const PERCENT = new RegExp(`^${DECIMAL}\\s*%$`);

/** Like `parseUnits`, also reading a percentage ("50%" is 0.5). */
export function parseUnitsOrPercent(text: string): number | null {
  const percent = PERCENT.exec(text.trim());
  if (!percent) return parseUnits(text);
  const units = decimal(percent[1]) / 100;
  return Number.isFinite(units) && units <= MAX_UNITS ? units : null;
}

/** Whether `text` is how `units` are shown, so leaving the field changes nothing. */
export function showsUnits(text: string, units: number): boolean {
  return text.trim() === formatUnits(units);
}

/** Units as typed back: the language's decimal mark, no grouping. */
export function formatUnits(units: number): string {
  const text = String(Math.round(units * 1e6) / 1e6);
  return getLang() === "en" ? text : text.replace(".", ",");
}

/** Units as hours of an eight-hour day: "2 giờ/ngày", "2 hrs/day". */
export function hoursPerDay(units: number): string {
  const shown = formatHours((units * MINUTES_PER_DAY) / 60) || "0";
  return pick(assignmentUnitsMessages).perDay(shown, shown === "1");
}
