import { describe, expect, it } from "vitest";

import { clampToGrid, moveInGrid } from "./matrixGrid";

describe("matrix grid keys", () => {
  it("moves one cell and stops at the edges", () => {
    expect(moveInGrid({ row: 1, col: 1 }, "ArrowUp", 3, 3)).toEqual({ row: 0, col: 1 });
    expect(moveInGrid({ row: 0, col: 1 }, "ArrowUp", 3, 3)).toEqual({ row: 0, col: 1 });
    expect(moveInGrid({ row: 2, col: 2 }, "ArrowDown", 3, 3)).toEqual({ row: 2, col: 2 });
    expect(moveInGrid({ row: 1, col: 0 }, "ArrowLeft", 3, 3)).toEqual({ row: 1, col: 0 });
    expect(moveInGrid({ row: 1, col: 1 }, "ArrowRight", 3, 3)).toEqual({ row: 1, col: 2 });
  });

  it("goes to the ends of the row, or of the grid with Ctrl/Cmd", () => {
    expect(moveInGrid({ row: 1, col: 1 }, "Home", 3, 4)).toEqual({ row: 1, col: 0 });
    expect(moveInGrid({ row: 1, col: 1 }, "End", 3, 4)).toEqual({ row: 1, col: 3 });
    expect(moveInGrid({ row: 1, col: 1 }, "Home", 3, 4, true)).toEqual({ row: 0, col: 0 });
    expect(moveInGrid({ row: 1, col: 1 }, "End", 3, 4, true)).toEqual({ row: 2, col: 3 });
  });

  it("ignores other keys and empty grids", () => {
    expect(moveInGrid({ row: 0, col: 0 }, "Enter", 3, 3)).toBeNull();
    expect(moveInGrid({ row: 0, col: 0 }, "ArrowDown", 0, 3)).toBeNull();
  });

  it("keeps a position inside a grid that shrank", () => {
    expect(clampToGrid({ row: 5, col: 5 }, 2, 3)).toEqual({ row: 1, col: 2 });
    expect(clampToGrid({ row: 1, col: 1 }, 0, 0)).toEqual({ row: 0, col: 0 });
  });
});
