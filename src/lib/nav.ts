export type NavKey = "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight";

export const isNavKey = (key: string): key is NavKey =>
  key === "ArrowUp" || key === "ArrowDown" || key === "ArrowLeft" || key === "ArrowRight";

/**
 * Which card Enter and Space act on (-1 for none). A card the pointer rests over counts once the pointer
 * has moved since the last key was pressed (typing, an arrow, Tab): the person is pointing at it, so there
 * is no need to click first, which is what a pen or touch screen with a keyboard needs. Without that, it is
 * the card chosen with the arrow keys. Moving the pointer after typing is the signal that makes
 * "type a search, then press Enter" safe: a card that merely sits under a resting cursor is never downloaded.
 */
export function actingIndex(input: { selected: number; hovered: number; pointerAt: number; keyAt: number }): number {
  return input.hovered >= 0 && input.pointerAt > input.keyAt ? input.hovered : input.selected;
}

/**
 * The next selected card for an arrow key in a grid with `columns` columns and `count` cards.
 * -1 means "no card selected", which hands focus back to the search box: Up from the first row
 * leaves the grid, and Down from the search box enters it.
 */
export function moveSelection(current: number, key: NavKey, columns: number, count: number): number {
  if (count <= 0) return -1;
  const cols = Math.max(1, columns);
  if (current < 0 || current >= count) return key === "ArrowDown" || key === "ArrowRight" ? 0 : -1;
  switch (key) {
    case "ArrowRight":
      return Math.min(current + 1, count - 1);
    case "ArrowLeft":
      return Math.max(current - 1, 0);
    case "ArrowDown": {
      if (current + cols < count) return current + cols;
      // A shorter last row: land on its final card rather than not moving.
      const onLastRow = Math.floor(current / cols) === Math.floor((count - 1) / cols);
      return onLastRow ? current : count - 1;
    }
    case "ArrowUp":
      return current - cols >= 0 ? current - cols : -1;
  }
}
