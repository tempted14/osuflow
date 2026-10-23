import type { Beatmapset, LibraryDelta } from "./types";

/** Appends a page of results, skipping sets that are already listed (mirrors can repeat them). */
export function mergeResults(existing: Beatmapset[], incoming: Beatmapset[]): Beatmapset[] {
  const seen = new Set(existing.map((set) => set.id));
  return [...existing, ...incoming.filter((set) => !seen.has(set.id))];
}

/**
 * What the grid shows. With "hide installed" on, sets already in the library disappear, except
 * ones with a download in progress or just finished, so a card doesn't vanish under the cursor.
 */
export function visibleResults(
  results: Beatmapset[],
  installed: ReadonlySet<number>,
  hideInstalled: boolean,
  keep: ReadonlySet<number>,
): Beatmapset[] {
  if (!hideInstalled) return results;
  return results.filter((set) => !installed.has(set.id) || keep.has(set.id));
}

export function applyLibraryDelta(current: ReadonlySet<number>, delta: LibraryDelta): ReadonlySet<number> {
  const next = new Set(current);
  delta.removed.forEach((id) => next.delete(id));
  delta.added.forEach((id) => next.add(id));
  return next;
}
