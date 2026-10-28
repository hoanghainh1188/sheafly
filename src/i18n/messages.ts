import { getLang, type Lang } from "./lang";

/** A message is plain text, or a function when it takes values (counts, names, dates). */
export type Message = string | ((...args: never[]) => string);
export type Messages = Record<string, Message>;

/** The same messages in every language; English and Japanese follow the Vietnamese keys. */
export type Catalog<T extends Messages> = Record<Lang, T>;

/**
 * Define one area's messages. `en` and `ja` are typed from `vi`, so a missing or extra key, or a
 * function with different parameters, is a type error. If `ja` is omitted, it falls back to `en`.
 */
export function defineMessages<T extends Messages>(
  vi: T,
  en: NoInfer<T>,
  ja?: NoInfer<T>,
): Catalog<T> {
  return { vi, en, ja: ja ?? en };
}

/** The messages for a language; outside React, the current one. */
export function pick<T extends Messages>(catalog: Catalog<T>, lang: Lang = getLang()): T {
  return catalog[lang];
}
