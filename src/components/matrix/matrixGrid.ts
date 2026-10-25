// Keyboard movement in the assignment matrix (a WAI-ARIA grid): one focusable value cell
// at a time, moved with the arrow keys, Home/End (row) and Ctrl/Cmd+Home/End (grid).

export interface GridPos {
  row: number;
  col: number;
}

/** Where `key` moves the focus from `at` in a grid of `rows` × `cols`, or null when it does not move it. */
export function moveInGrid(
  at: GridPos,
  key: string,
  rows: number,
  cols: number,
  toEdge = false,
): GridPos | null {
  if (rows === 0 || cols === 0) return null;
  const clamp = (value: number, size: number) => Math.min(Math.max(value, 0), size - 1);
  switch (key) {
    case "ArrowUp":
      return { row: clamp(at.row - 1, rows), col: at.col };
    case "ArrowDown":
      return { row: clamp(at.row + 1, rows), col: at.col };
    case "ArrowLeft":
      return { row: at.row, col: clamp(at.col - 1, cols) };
    case "ArrowRight":
      return { row: at.row, col: clamp(at.col + 1, cols) };
    case "Home":
      return toEdge ? { row: 0, col: 0 } : { row: at.row, col: 0 };
    case "End":
      return toEdge ? { row: rows - 1, col: cols - 1 } : { row: at.row, col: cols - 1 };
    default:
      return null;
  }
}

/** `at` kept inside a grid that may have shrunk since. */
export function clampToGrid(at: GridPos, rows: number, cols: number): GridPos {
  return {
    row: Math.min(at.row, Math.max(rows - 1, 0)),
    col: Math.min(at.col, Math.max(cols - 1, 0)),
  };
}
