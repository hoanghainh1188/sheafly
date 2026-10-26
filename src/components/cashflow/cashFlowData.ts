import { dateKeyToInstant, formatDateOnly } from "../../format";
import { pick } from "../../i18n";
import type { Bucket, CashFlowPeriod, CashFlowReport, Flows } from "../../types";
import { cashFlowMessages } from "./CashFlowView.i18n";

/** Which of the three series the view shows. */
export type CashSeries = "forecast" | "actual" | "baseline";

/** One period's figures in one series; null where the series has none (actual after the status date). */
export interface PeriodFigures {
  inflow: number | null;
  outflow: number | null;
  net: number | null;
  balance: number | null;
}

function of(flows: Flows, series: CashSeries): number | null {
  return series === "forecast" ? flows.forecast : flows[series];
}

export function figures(period: CashFlowPeriod, series: CashSeries): PeriodFigures {
  return {
    inflow: of(period.inflow, series),
    outflow: of(period.outflow, series),
    net: of(period.net, series),
    balance: of(period.balance, series),
  };
}

/** A period's name by its bucket: the day, "Tuần 05/01/2026", "01/2026" or "Quý 1/2026". */
export function periodLabel(label: string, bucket: Bucket): string {
  const m = pick(cashFlowMessages);
  const at = dateKeyToInstant(label);
  if (at == null) return label;
  const [year, month] = [Number(label.slice(0, 4)), Number(label.slice(5, 7))];
  if (bucket === "day") return formatDateOnly(at);
  if (bucket === "week") return m.weekOf(formatDateOnly(at));
  if (bucket === "quarter") return m.quarterOf(Math.floor((month - 1) / 3) + 1, year);
  return `${String(month).padStart(2, "0")}/${year}`;
}

/** The value range a chart of `series` needs, always holding zero. */
export function valueRange(report: CashFlowReport, series: CashSeries): { low: number; high: number } {
  let [low, high] = [0, 0];
  for (const period of report.periods) {
    const f = figures(period, series);
    high = Math.max(high, f.inflow ?? 0, f.balance ?? 0);
    low = Math.min(low, -(f.outflow ?? 0), f.balance ?? 0);
  }
  if (high === low) high = low + 1;
  return { low, high };
}

/** Rounded ticks across `low`–`high`, zero among them. */
export function ticks(low: number, high: number, count = 5): number[] {
  const raw = (high - low) / count;
  const power = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 2.5, 5, 10].map((f) => f * power).find((s) => s >= raw) ?? raw;
  const out: number[] = [];
  for (let value = Math.ceil(low / step) * step; value <= high + step * 1e-9; value += step) out.push(Math.round(value / step) * step);
  return out;
}
