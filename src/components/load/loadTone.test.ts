import { describe, expect, it } from "vitest";

import { loadTone } from "./loadTone";

describe("loadTone", () => {
  it("colours a cell by its share of capacity", () => {
    expect(loadTone({ work: 0, capacity: 0 })).toBe("off");
    expect(loadTone({ work: 0, capacity: 480 })).toBe("free");
    expect(loadTone({ work: 240, capacity: 480 })).toBe("light");
    expect(loadTone({ work: 384, capacity: 480 })).toBe("full");
    expect(loadTone({ work: 480, capacity: 480 })).toBe("full");
    expect(loadTone({ work: 481, capacity: 480 })).toBe("over");
    expect(loadTone({ work: 60, capacity: 0 })).toBe("over");
    expect(loadTone({ work: 240, unassigned: 240, capacity: 480 })).toBe("full");
    expect(loadTone({ work: 240, unassigned: 250, capacity: 480 })).toBe("over");
  });
});
