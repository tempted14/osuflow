/** Keyboard behaviour of the flyout menus, kept apart from the component so it can be tested on its own. */

/** The keys that move the highlight in a menu. */
export const MENU_NAV_KEYS = ["ArrowDown", "ArrowUp", "PageDown", "PageUp", "Home", "End"];

/**
 * Where the highlight goes for a navigation key: a step, a page of five, or an end. It stops at the ends
 * instead of wrapping round, as Windows' own lists do. Any other key leaves it where it is.
 */
export function stepIndex(current: number, key: string, count: number): number {
  if (count <= 0) return -1;
  const last = count - 1;
  switch (key) {
    case "ArrowDown":
      return Math.min(current + 1, last);
    case "ArrowUp":
      return Math.max(current - 1, 0);
    case "PageDown":
      return Math.min(current + 5, last);
    case "PageUp":
      return Math.max(current - 5, 0);
    case "Home":
      return 0;
    case "End":
      return last;
    default:
      return current;
  }
}

/**
 * Type-ahead: the first entry whose label starts with what was typed (-1 for none). One letter typed
 * again and again steps through the entries that start with it.
 */
export function typeAhead(labels: readonly string[], typed: string, current: number): number {
  const wanted = typed.toLowerCase();
  if (!wanted) return -1;
  const starts = (index: number) => labels[index].toLowerCase().startsWith(wanted[0]);
  if ([...wanted].every((char) => char === wanted[0])) {
    for (let step = 1; step <= labels.length; step++) {
      const index = (current + step) % labels.length;
      if (starts(index)) return index;
    }
    return -1;
  }
  return labels.findIndex((label) => label.toLowerCase().startsWith(wanted));
}
