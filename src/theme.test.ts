import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import css from "./tokens.css?raw";
import { applyTheme, CANVAS_TOKENS, LIGHT_CANVAS, resolvedTheme, useResolvedTheme } from "./theme";


function tokens(selector: string): Map<string, string> {
  const start = css.indexOf(`${selector} {`);
  const block = css.slice(start, css.indexOf("\n}", start));
  return new Map([...block.matchAll(/(--[\w-]+):\s*([^;]+);/g)].map((match) => [match[1], match[2].trim()]));
}

const light = tokens(":root");
const dark = new Map([...light, ...tokens(':root[data-theme="dark"]')]);

function luminance(hex: string): number {
  const value = hex.replace("#", "");
  const [r, g, b] = [0, 2, 4].map((offset) => {
    const channel = Number.parseInt(value.slice(offset, offset + 2), 16) / 255;
    return channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function colour(palette: Map<string, string>, token: string): string {
  const value = palette.get(token);
  if (!value?.startsWith("#")) throw new Error(`${token} is not a hex colour: ${value}`);
  return value;
}

function contrast(palette: Map<string, string>, fg: string, bg: string): number {
  const a = luminance(colour(palette, fg));
  const b = luminance(colour(palette, bg));
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

// Text and the surfaces it sits on (12 px text: WCAG AA asks for 4.5:1).
const TEXT_PAIRS: [string, string[]][] = [
  ["--text", ["--surface", "--surface-app", "--surface-chrome", "--surface-sunken", "--surface-muted", "--surface-group", "--selected-row", "--highlight-row", "--highlight-selected", "--network-box", "--network-summary-box", "--network-highlight-box"]],
  ["--text-secondary", ["--surface", "--surface-chrome", "--surface-sunken", "--surface-muted", "--surface-group", "--segmented-bg"]],
  ["--text-tab", ["--surface-chrome", "--tab-hover"]],
  ["--text-muted", ["--surface", "--surface-chrome", "--surface-sunken", "--surface-muted", "--chart-bg"]],
  ["--text-faint", ["--surface", "--surface-sunken", "--surface-muted", "--weekend", "--network-bg"]],
  ["--critical-text", ["--surface", "--selected-row", "--highlight-row", "--tone-danger-bg"]],
  ["--banner-text", ["--banner-bg"]],
  ["--over-text", ["--over-bg"]],
  ["--on-accent", ["--accent-strong"]],
  ["--tone-danger", ["--surface"]],
  ["--pro-badge", ["--pro-badge-bg"]],
  ["--matrix-rule", ["--matrix-rule-bg", "--surface-sunken"]],
  ["--text", ["--matrix-assigned-bg"]],
  ...["history", "create", "edit", "danger", "project", "report"].flatMap(
    (tone): [string, string[]][] => [
      [`--tone-${tone}`, [`--tone-${tone}-bg`]],
      ["--on-tone", [`--tone-${tone}`]],
    ],
  ),
];

describe("theme tokens", () => {
  afterEach(() => {
    applyTheme("system");
    vi.unstubAllGlobals();
  });

  it.each([
    ["light", light],
    ["dark", dark],
  ])("keeps WCAG AA text contrast in the %s theme", (_name, palette) => {
    const failures: string[] = [];
    for (const [fg, backgrounds] of TEXT_PAIRS) {
      for (const bg of backgrounds) {
        const ratio = contrast(palette, fg, bg);
        if (ratio < 4.5) failures.push(`${fg} on ${bg}: ${ratio.toFixed(2)}`);
      }
    }
    expect(failures).toEqual([]);
  });

  it.each([
    ["light", light],
    ["dark", dark],
  ])("keeps bars, links and the focus ring visible in the %s theme (3:1)", (_name, palette) => {
    for (const fg of ["--bar-task", "--bar-critical", "--bar-summary", "--link", "--accent"]) {
      expect(contrast(palette, fg, "--surface"), fg).toBeGreaterThanOrEqual(3);
    }
  });

  it("defines every token in both themes, and the canvas defaults match the light theme", () => {
    const darkOnly = tokens(':root[data-theme="dark"]');
    const colours = [...light.keys()].filter((key) => !["--font-ui", "--scale-head"].includes(key));
    expect(colours.filter((key) => !darkOnly.has(key))).toEqual([]);
    for (const [key, [token, value]] of Object.entries(CANVAS_TOKENS)) {
      expect(light.get(token), key).toBe(value);
      expect(LIGHT_CANVAS[key as keyof typeof LIGHT_CANVAS]).toBe(value);
    }
  });

  it("sets data-theme from the choice and follows the system for System", () => {
    let systemDark = true;
    const listeners = new Set<() => void>();
    vi.stubGlobal("matchMedia", (query: string) => ({
      get matches() {
        return query.includes("dark") && systemDark;
      },
      addEventListener: (_type: string, listener: () => void) => listeners.add(listener),
      removeEventListener: (_type: string, listener: () => void) => listeners.delete(listener),
    }));
    const { result } = renderHook(() => useResolvedTheme());
    act(() => applyTheme("light"));
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(result.current).toBe("light");
    act(() => applyTheme("system"));
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(result.current).toBe("dark");
    // The system turns light while System is chosen.
    systemDark = false;
    act(() => listeners.forEach((listener) => listener()));
    expect(resolvedTheme()).toBe("light");
    act(() => applyTheme("dark"));
    expect(result.current).toBe("dark");
    act(() => applyTheme("light"));
  });
});
