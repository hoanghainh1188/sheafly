import { getLang } from "./i18n/lang";

/** The engine's range (`focus::MIN_FOCUS`, `MAX_FOCUS`). */
export const MIN_FOCUS = 0.5;
export const MAX_FOCUS = 1;

/** A productivity factor as a percentage: 0.8 -> "80%", 0.825 -> "82,5%" ("82.5%"). */
export function formatFocus(factor: number): string {
  const percent = Math.round(factor * 1000) / 10;
  const text = Number.isInteger(percent) ? String(percent) : percent.toFixed(1);
  return `${getLang() === "en" ? text : text.replace(".", ",")}%`;
}

/**
 * A factor typed as a percentage ("80", "80%", "82,5 %") or a fraction ("0,8", "0.8"),
 * with either decimal mark; `null` when it does not read or lies outside 50–100%.
 */
export function parseFocus(text: string): number | null {
  const cleaned = text.trim().replace(/\s*%$/, "").replace(",", ".");
  if (!/^\d+(\.\d+)?$/.test(cleaned)) return null;
  const value = Number(cleaned);
  const factor = value <= 1 && !text.includes("%") ? value : value / 100;
  // Rounded so "82,5" does not reach the engine as 0.8250000000000001.
  const rounded = Math.round(factor * 10000) / 10000;
  return rounded >= MIN_FOCUS && rounded <= MAX_FOCUS ? rounded : null;
}
