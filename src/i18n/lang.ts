/** The interface languages. Vietnamese is the original; English and Japanese follow key for key. */
export type Lang = "vi" | "en" | "ja";

export const LANGS: readonly Lang[] = ["vi", "en", "ja"];

export function isLang(value: unknown): value is Lang {
  return value === "vi" || value === "en" || value === "ja";
}

// The language the format helpers (dates, durations, money) read. I18nProvider keeps it in
// step with the language React renders, so plain functions need no extra argument.
// Vietnamese by default, which keeps tests that do not set a language unchanged.
let current: Lang = "vi";

export function getLang(): Lang {
  return current;
}

export function setCurrentLang(lang: Lang): void {
  current = lang;
}

/** First launch: Vietnamese or Japanese when the system prefers it, English otherwise. */
export function detectLang(preferred: readonly string[]): Lang {
  const first = (preferred[0] ?? "").toLowerCase();
  if (first === "vi" || first.startsWith("vi-")) return "vi";
  if (first === "ja" || first.startsWith("ja-")) return "ja";
  return "en";
}
