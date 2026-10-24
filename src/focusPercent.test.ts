import { afterEach, describe, expect, it } from "vitest";

import { setCurrentLang } from "./i18n";
import { formatFocus, parseFocus } from "./focusPercent";

afterEach(() => setCurrentLang("vi"));

describe("focusPercent", () => {
  it("reads percentages and fractions with either decimal mark", () => {
    expect(parseFocus("80")).toBe(0.8);
    expect(parseFocus(" 80 % ")).toBe(0.8);
    expect(parseFocus("82,5")).toBe(0.825);
    expect(parseFocus("0,75")).toBe(0.75);
    expect(parseFocus("0.75")).toBe(0.75);
    expect(parseFocus("1")).toBe(1);
    expect(parseFocus("100%")).toBe(1);
  });

  it("refuses what lies outside 50–100% or does not read", () => {
    for (const text of ["49", "101", "0,4", "1%", "", "abc", "-80"]) expect(parseFocus(text)).toBeNull();
  });

  it("shows the language's decimal mark", () => {
    expect(formatFocus(0.8)).toBe("80%");
    expect(formatFocus(0.825)).toBe("82,5%");
    setCurrentLang("en");
    expect(formatFocus(0.825)).toBe("82.5%");
  });
});
