import { getLang } from "../../i18n/lang";
import type { Payment, PaymentAnchor, PaymentInput, Project, Task } from "../../types";

/** The engine's limits (`payments::MAX_LAG_DAYS`, `MAX_RETENTION_PERCENT`). */
export const MAX_LAG_DAYS = 365;
export const MAX_RETENTION_PERCENT = 50;
/** Net 30, the usual invoice term. */
export const DEFAULT_LAG_DAYS = 30;

/** A percent as typed: "30", "30%", "12,5 %" or "12.5"; null when it does not read or is above `max`. */
export function parsePercent(text: string, max = 100): number | null {
  const cleaned = text.trim().replace(/\s*%$/, "").replace(",", ".");
  if (!/^\d+(\.\d+)?$/.test(cleaned)) return null;
  // Rounded so "12,5" does not reach the engine as 12.500000000000002.
  const value = Math.round(Number(cleaned) * 10000) / 10000;
  return value <= max ? value : null;
}

/** 30 -> "30%", 12.5 -> "12,5%" ("12.5%" in English). */
export function formatPercent(value: number): string {
  const rounded = Math.round(value * 100) / 100;
  const text = String(rounded);
  return `${getLang() === "en" ? text : text.replace(".", ",")}%`;
}

/** Whole calendar days from invoice to due date, 0 to 365; null otherwise. */
export function parseLagDays(text: string): number | null {
  const cleaned = text.trim();
  if (!/^\d+$/.test(cleaned)) return null;
  const days = Number(cleaned);
  return days <= MAX_LAG_DAYS ? days : null;
}

/** What `SetPayments` takes back for a payment as it is. */
export function toInput(payment: Payment): PaymentInput {
  return {
    id: payment.id,
    name: payment.name,
    amount: payment.amount,
    anchor: payment.anchor,
    lag_days: payment.lag_days,
    retained: payment.retained,
  };
}

/** Tasks a payment can follow: milestones and groups first, then the other tasks, each in outline order. */
export function anchorTasks(project: Project): Task[] {
  const first = project.tasks.filter((task) => task.milestone || task.summary);
  const rest = project.tasks.filter((task) => !task.milestone && !task.summary);
  return [...first, ...rest];
}

/** Where a new payment goes: the last milestone, else the first group, else the first task, else the project start. */
export function defaultAnchor(project: Project): PaymentAnchor {
  const milestones = project.tasks.filter((task) => task.milestone);
  const task = milestones[milestones.length - 1] ?? project.tasks.find((item) => item.summary) ?? project.tasks[0];
  return task ? { type: "finish", task: task.uid } : { type: "date", at: project.start };
}

/** The sum of the percent payments, or null when there is none. */
export function percentTotal(payments: Payment[]): number | null {
  const percents = payments.flatMap((payment) => ("percent" in payment.amount ? [payment.amount.percent] : []));
  return percents.length > 0 ? Math.round(percents.reduce((sum, value) => sum + value, 0) * 100) / 100 : null;
}

/** Whether a payment or the retention's release follows `uid`. */
export function followsTask(anchor: PaymentAnchor, uid: number): boolean {
  return anchor.type !== "date" && anchor.task === uid;
}
