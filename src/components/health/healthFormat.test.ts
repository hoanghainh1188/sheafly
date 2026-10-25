import { afterEach, describe, expect, it } from "vitest";

import { setCurrentLang } from "../../i18n";
import { makeHealthCriterion } from "../../test/fixtures";
import { formatHealthLimit, formatHealthValue, reasonCounts } from "./healthFormat";

describe("health check numbers", () => {
  afterEach(() => setCurrentLang("vi"));

  it("shows shares as percentages and 13, 14 as indexes, in the language's marks", () => {
    expect(formatHealthValue(1, 0.045)).toMatch(/^4,5\s?%$/);
    expect(formatHealthValue(14, 0.5)).toBe("0,50");
    setCurrentLang("en");
    expect(formatHealthValue(1, 0.045)).toBe("4.5%");
    expect(formatHealthValue(13, 1.2)).toBe("1.20");
  });

  it("shows a limit with its direction, and none allowed as 0", () => {
    setCurrentLang("en");
    expect(formatHealthLimit(1, { kind: "at_most", value: 0.05 })).toBe("≤ 5%");
    expect(formatHealthLimit(4, { kind: "at_least", value: 0.9 })).toBe("≥ 90%");
    expect(formatHealthLimit(14, { kind: "at_least", value: 0.95 })).toBe("≥ 0.95");
    expect(formatHealthLimit(2, { kind: "at_most", value: 0 })).toBe("0");
  });

  it("counts tasks by reason in the order reasons first appear", () => {
    const criterion = makeHealthCriterion(1, {
      tasks: [
        { uid: 1, reason: "no_successor" },
        { uid: 2, reason: "no_links" },
        { uid: 3, reason: "no_successor" },
      ],
    });
    expect(reasonCounts(criterion)).toEqual([
      ["no_successor", 2],
      ["no_links", 1],
    ]);
  });
});
