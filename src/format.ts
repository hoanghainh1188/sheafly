import { getLang } from "./i18n/lang";

// Text shown follows the interface language: Vietnamese "05/01/2026 08:00", "7,5 ngày",
// "3.480.000"; English "2026-01-05 08:00", "7.5 days", "3,480,000". Durations parse in
// either language. Dates: Vietnamese accepts its day-first form and ISO; English accepts
// only ISO, because an English reader may mean "1/5/2026" as January 5.

const DAY = 8 * 60;

/** The working minutes durations count as a day, as the engine's `MINUTES_PER_DAY`. */
export const MINUTES_PER_DAY = DAY;

/** Hundredths of a unit as text: 150 -> "1,5" ("1.5" in English), 75 -> "0,75". */
function hundredths(value: number): string {
  const whole = Math.floor(value / 100);
  const fraction = value % 100;
  if (fraction === 0) return String(whole);
  const point = getLang() === "vi" ? "," : ".";
  return `${whole}${point}${String(fraction).padStart(2, "0").replace(/0$/, "")}`;
}

const UNITS = {
  vi: { day: () => "ngày", hour: () => "giờ", minute: () => "phút" },
  en: {
    day: (one: boolean) => (one ? "day" : "days"),
    hour: (one: boolean) => (one ? "hr" : "hrs"),
    minute: (one: boolean) => (one ? "min" : "mins"),
  },
  ja: {
    day: () => "日",
    hour: () => "時間",
    minute: () => "分",
  },
};

/**
 * Working minutes as days, the way Project shows durations ("7,5 ngày", "7.5 days", or "7.5 日", a
 * milestone "0 ngày", "0 days", or "0 日"). Falls back to hours, then minutes, only when days with
 * two decimals would not be exact, so text shown in an editor parses back to the same
 * number of minutes.
 */
export function formatDuration(minutes: number): string {
  if (!Number.isFinite(minutes)) return "";
  const units = UNITS[getLang()];
  const sign = minutes < 0 ? "-" : "";
  const abs = Math.abs(Math.round(minutes));
  if (abs === 0) return `0 ${units.day(false)}`;
  if ((abs * 100) % DAY === 0) return `${sign}${hundredths((abs * 100) / DAY)} ${units.day(abs === DAY)}`;
  if ((abs * 100) % 60 === 0) return `${sign}${hundredths((abs * 100) / 60)} ${units.hour(abs === 60)}`;
  return `${sign}${abs} ${units.minute(abs === 1)}`;
}

const DAY_UNITS = new Set(["d", "ngày", "ngay", "day", "days", "日"]);
const HOUR_UNITS = new Set(["h", "giờ", "gio", "hr", "hrs", "hour", "hours", "時間"]);
const MINUTE_UNITS = new Set(["m", "phút", "phut", "min", "mins", "minute", "minutes", "分"]);

/** Duration text in either language; a bare number is days. */
export function parseDuration(text: string): number | null {
  const cleaned = text.trim().toLowerCase().replace(",", ".");
  if (!cleaned) return null;
  const match = cleaned.match(/^(-?\d+(?:\.\d+)?)\s*([a-zàáạảãâầấậẩẫăằắặẳẵèéẹẻẽêềếệểễìíịỉĩòóọỏõôồốộổỗơờớợởỡùúụủũưừứựửữỳýỵỷỹđ\u3040-\u30ff\u4e00-\u9faf]+)?$/);
  if (!match) return null;
  const value = Number(match[1]);
  const unit = match[2];
  if (unit === undefined || DAY_UNITS.has(unit)) return Math.round(value * DAY);
  if (HOUR_UNITS.has(unit)) return Math.round(value * 60);
  if (MINUTE_UNITS.has(unit)) return Math.round(value);
  return null;
}

function pad2(value: number): string {
  return String(value).padStart(2, "0");
}

/**
 * Lag units the English interface shows but the engine's predecessor parser does not read
 * ("2SS+2 hrs", "+30 mins") become the short forms it does ("2SS+2h", "+30m"). Normalized
 * here rather than in the engine, so a project saved by this app still replays in an older
 * one.
 */
export function normalizePredecessorText(text: string): string {
  return text
    .replace(/(\d)\s*(?:hrs|hours?|hr|時間)\b/gi, "$1h")
    .replace(/(\d)\s*(?:mins|minutes?|min|分)\b/gi, "$1m")
    .replace(/(\d)\s*(?:days?|日)\b/gi, "$1d");
}

export function formatInstant(ms: number): string {
  const date = new Date(ms);
  const time = `${pad2(date.getUTCHours())}:${pad2(date.getUTCMinutes())}`;
  return `${formatDateOnly(ms)} ${time}`;
}

/** Day, month, year from "2026-01-05", or in Vietnamese also "05/01/2026". */
function parseDateParts(text: string): { day: number; month: number; year: number } | null {
  const dayFirst = getLang() === "vi" ? text.match(/^(\d{1,2})\/(\d{1,2})\/(\d{4})$/) : null;
  if (dayFirst) return { day: Number(dayFirst[1]), month: Number(dayFirst[2]), year: Number(dayFirst[3]) };
  const iso = text.match(/^(\d{4})[-/](\d{1,2})[-/](\d{1,2})$/);
  if (iso) return { day: Number(iso[3]), month: Number(iso[2]), year: Number(iso[1]) };
  return null;
}

/** "2026-01-05 08:00" (or "05/01/2026 08:00" in Vietnamese); without a time, 08:00. */
export function parseInstant(text: string): number | null {
  const match = text.trim().match(/^(\S+)(?:\s+(\d{1,2}):(\d{2}))?$/);
  if (!match) return null;
  const parts = parseDateParts(match[1]);
  if (!parts) return null;
  const { day, month, year } = parts;
  const hour = match[2] ? Number(match[2]) : 8;
  const minute = match[3] ? Number(match[3]) : 0;
  if (hour > 23 || minute > 59) return null;
  const ms = Date.UTC(year, month - 1, day, hour, minute);
  if (Number.isNaN(ms)) return null;
  const check = new Date(ms);
  // Reject dates like 31/02 that Date.UTC would silently roll over.
  if (check.getUTCDate() !== day || check.getUTCMonth() !== month - 1) return null;
  return ms;
}

export function formatDateOnly(ms: number): string {
  const key = instantToDateKey(ms);
  return getLang() === "vi" ? formatDateKeyDisplay(key) : key;
}

// Date-only text maps to UTC midnight of that wall-clock day.
export function parseDateOnly(text: string): number | null {
  if (!parseDateParts(text.trim())) return null;
  return parseInstant(`${text.trim()} 00:00`);
}

export function dateKeyToInstant(key: string): number | null {
  const match = key.match(/^(\d{4})-(\d{2})-(\d{2})$/);
  if (!match) return null;
  const ms = Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3]));
  return Number.isNaN(ms) ? null : ms;
}

export function instantToDateKey(ms: number): string {
  const date = new Date(ms);
  return `${date.getUTCFullYear()}-${pad2(date.getUTCMonth() + 1)}-${pad2(date.getUTCDate())}`;
}

// Time-of-day in minutes; 24:00 is allowed because working ranges may end at midnight.
export function parseTimeOfDay(text: string): number | null {
  const match = text.trim().match(/^(\d{1,2})(?::|h)?(\d{2})$/);
  if (!match) return null;
  const hour = Number(match[1]);
  const minute = Number(match[2]);
  if (minute > 59 || hour > 24 || (hour === 24 && minute !== 0)) return null;
  return hour * 60 + minute;
}

// Today's wall-clock date in the user's timezone, as UTC midnight to match project instants.
export function todayInstant(): number {
  const now = new Date();
  return Date.UTC(now.getFullYear(), now.getMonth(), now.getDate());
}

// Project wall-clock is Unix milliseconds shown with UTC fields, matching the engine.
export function instantToDatetimeLocal(ms: number): string {
  const date = new Date(ms);
  return `${date.getUTCFullYear()}-${pad2(date.getUTCMonth() + 1)}-${pad2(date.getUTCDate())}T${pad2(date.getUTCHours())}:${pad2(date.getUTCMinutes())}`;
}

export function datetimeLocalToInstant(value: string): number | null {
  const match = value.trim().match(/^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})/);
  if (!match) return null;
  const ms = Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3]), Number(match[4]), Number(match[5]));
  return Number.isNaN(ms) ? null : ms;
}

export function minuteToTime(minute: number): string {
  const clamped = Math.max(0, Math.min(24 * 60, Math.round(minute)));
  return `${pad2(Math.floor(clamped / 60))}:${pad2(clamped % 60)}`;
}

export function timeToMinute(value: string): number | null {
  const match = value.trim().match(/^(\d{2}):(\d{2})/);
  if (!match) return null;
  const hour = Number(match[1]);
  const minute = Number(match[2]);
  if (hour > 23 || minute > 59) return null;
  return hour * 60 + minute;
}

/** A calendar key ("2026-01-05") as shown: "05/01/2026" in Vietnamese, unchanged in English or Japanese. */
export function formatDateKeyDisplay(key: string): string {
  const match = key.match(/^(\d{4})-(\d{2})-(\d{2})$/);
  if (!match || getLang() !== "vi") return key;
  return `${match[3]}/${match[2]}/${match[1]}`;
}

/** Typed date ("2026-01-05", or "05/01/2026" in Vietnamese) to a calendar key. */
export function formatDateKey(text: string): string | null {
  const parts = parseDateParts(text.trim());
  if (!parts) return null;
  return `${parts.year}-${pad2(parts.month)}-${pad2(parts.day)}`;
}

export function formatMoney(value: number): string {
  const lang = getLang();
  const locale = lang === "en" ? "en-US" : lang === "ja" ? "ja-JP" : "vi-VN";
  return new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(Math.round(value));
}

/** An amount as a money field shows it: the language's grouping, up to two decimals. */
export function formatMoneyInput(value: number): string {
  const lang = getLang();
  const locale = lang === "en" ? "en-US" : lang === "ja" ? "ja-JP" : "vi-VN";
  return new Intl.NumberFormat(locale, { maximumFractionDigits: lang === "ja" ? 0 : 2 }).format(value);
}

/**
 * An amount typed in a money field: grouped the way the language shows money ("1.248.000,5"
 * in Vietnamese, "1,248,000.5" in English and Japanese), or plain with either decimal mark ("12,5",
 * "12.5"); spaces are ignored. Null for anything else, negatives included.
 */
export function parseMoney(text: string): number | null {
  const cleaned = text.replace(/\s/g, "");
  const vi = getLang() === "vi";
  const grouped = vi ? /^\d{1,3}(\.\d{3})+(,\d+)?$/ : /^\d{1,3}(,\d{3})+(\.\d+)?$/;
  if (grouped.test(cleaned)) {
    const [whole, part] = cleaned.split(vi ? "," : ".");
    return Number(`${whole.replace(/[.,]/g, "")}${part ? `.${part}` : ""}`);
  }
  if (/^\d+([.,]\d+)?$/.test(cleaned)) return Number(cleaned.replace(",", "."));
  return null;
}

/** Hours with at most one decimal, with the language's decimal mark ("2,5" in Vietnamese, "2.5" in English/Japanese). */
export function formatHours(value: number): string {
  if (Math.abs(value) < 0.05) return "";
  const rounded = Math.round(value * 10) / 10;
  if (Number.isInteger(rounded)) return String(rounded);
  const text = rounded.toFixed(1);
  return getLang() === "vi" ? text.replace(".", ",") : text;
}

export function shiftDays(ms: number, days: number): number {
  const date = new Date(ms);
  return Date.UTC(
    date.getUTCFullYear(),
    date.getUTCMonth(),
    date.getUTCDate() + days,
    date.getUTCHours(),
    date.getUTCMinutes(),
  );
}

export function utcMidnight(ms: number): number {
  const date = new Date(ms);
  return Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate());
}
