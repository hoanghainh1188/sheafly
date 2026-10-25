import { afterEach, describe, expect, it } from "vitest";

import { formatUnits, hoursPerDay, parseUnits, parseUnitsOrPercent } from "./assignmentUnits";
import { setCurrentLang } from "./i18n";

describe("assignment units", () => {
  afterEach(() => setCurrentLang("vi"));

  it("reads plain units with either decimal mark", () => {
    expect(parseUnits("1")).toBe(1);
    expect(parseUnits(" 0,5 ")).toBe(0.5);
    expect(parseUnits("0.25")).toBe(0.25);
    expect(parseUnits("2")).toBe(2);
  });

  it("reads hours per day against an eight-hour day, in either language", () => {
    expect(parseUnits("2h/ngày")).toBe(0.25);
    expect(parseUnits("2 giờ / ngày")).toBe(0.25);
    expect(parseUnits("1,5 giờ/ngày")).toBe(0.1875);
    expect(parseUnits("4h/day")).toBe(0.5);
    expect(parseUnits("2 hrs/day")).toBe(0.25);
    expect(parseUnits("6 hours/day")).toBe(0.75);
    expect(parseUnits("8H/NGÀY")).toBe(1);
  });

  it("refuses what is not units", () => {
    for (const text of ["", "abc", "-1", "2h", "2h/tuần", "1e3", "1.000,5", "Infinity", "99999999"]) {
      expect(parseUnits(text), text).toBeNull();
    }
  });

  it("shows units with the language's decimal mark and as hours per day", () => {
    expect(formatUnits(0.25)).toBe("0,25");
    expect(hoursPerDay(0.25)).toBe("2 giờ/ngày");
    expect(hoursPerDay(0.1875)).toBe("1,5 giờ/ngày");
    setCurrentLang("en");
    expect(formatUnits(0.25)).toBe("0.25");
    expect(hoursPerDay(0.25)).toBe("2 hrs/day");
    expect(hoursPerDay(0.125)).toBe("1 hr/day");
    expect(hoursPerDay(0.124)).toBe("1 hr/day");
    expect(hoursPerDay(0.13)).toBe("1 hr/day");
  });

  it("reads a percentage as units where the field allows it", () => {
    expect(parseUnitsOrPercent("50%")).toBe(0.5);
    expect(parseUnitsOrPercent(" 12,5 % ")).toBe(0.125);
    expect(parseUnitsOrPercent("0%")).toBe(0);
    expect(parseUnitsOrPercent("0,75")).toBe(0.75);
    expect(parseUnitsOrPercent("abc%")).toBeNull();
    expect(parseUnits("50%")).toBeNull();
  });
});
