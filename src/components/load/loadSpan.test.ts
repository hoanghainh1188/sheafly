import { describe, expect, it } from "vitest";

import { loadSpanEnd } from "./loadSpan";

describe("loadSpanEnd", () => {
  it("counts days from the start", () => {
    expect(loadSpanEnd("2026-01-05", "day", 1)).toBe("2026-01-05");
    expect(loadSpanEnd("2026-01-05", "day", 30)).toBe("2026-02-03");
  });

  it("ends weeks on a Sunday, the first one partial when the start is not a Monday", () => {
    expect(loadSpanEnd("2026-01-05", "week", 2)).toBe("2026-01-18");
    // Wednesday: two weeks are the one it is in and the next.
    expect(loadSpanEnd("2026-01-07", "week", 2)).toBe("2026-01-18");
    expect(loadSpanEnd("2026-01-11", "week", 1)).toBe("2026-01-11");
  });

  it("ends months on the last day of a calendar month, whatever day the start is", () => {
    expect(loadSpanEnd("2026-08-31", "month", 6)).toBe("2027-01-31");
    expect(loadSpanEnd("2026-01-31", "month", 1)).toBe("2026-01-31");
    expect(loadSpanEnd("2026-01-15", "month", 2)).toBe("2026-02-28");
    expect(loadSpanEnd("2027-12-01", "month", 3)).toBe("2028-02-29");
  });

  it("leaves a date it cannot read as it is", () => {
    expect(loadSpanEnd("", "week", 4)).toBe("");
  });
});
