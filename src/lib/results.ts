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

/**
 * How long fetched results count as fresh. It matches the backend's search cache, so asking
 * sooner could only get the same answer back.
 */
export const FRESH_FOR_MS = 60_000;

/** What the search last fetched successfully, and for which query. */
export interface FetchRecord {
  key: string;
  /** A monotonic timestamp (performance.now), so a clock change can't make old results look new. */
  at: number;
}

/** Two searches with the same key ask for the same thing (the backend ignores case and padding too). */
export function searchKey(params: { text: string; mode: number | null; status: number; sort: string }): string {
  return JSON.stringify([params.text.trim().toLowerCase(), params.mode, params.status, params.sort]);
}

export type FetchPlan = "skip" | "now" | "debounced";

export interface FetchSituation {
  /** The overlay is showing. While it is hidden the first search is fetched ahead of time, and nothing more. */
  enabled: boolean;
  /** The text changed since the last time this was asked: the user is typing. */
  typing: boolean;
  last: FetchRecord | null;
  /** The query whose first page is on its way right now, if any. */
  inFlightKey: string | null;
  key: string;
  now: number;
  hadError: boolean;
}

/**
 * Whether the first page of results needs fetching. The overlay should open with results already
 * there, so the first search runs while it is still hidden; after that a re-open only refetches
 * when the results are stale, and a fetch that is already on its way is left to finish instead of
 * being thrown away and started again.
 */
export function planFetch(situation: FetchSituation): FetchPlan {
  const { enabled, typing, last, inFlightKey, key, now, hadError } = situation;
  if (!enabled) return last === null && inFlightKey === null ? "now" : "skip";
  if (typing) return "debounced";
  if (inFlightKey === key) return "skip";
  const fresh = last !== null && last.key === key && now - last.at < FRESH_FOR_MS && !hadError;
  return fresh ? "skip" : "now";
}

export function applyLibraryDelta(current: ReadonlySet<number>, delta: LibraryDelta): ReadonlySet<number> {
  const next = new Set(current);
  delta.removed.forEach((id) => next.delete(id));
  delta.added.forEach((id) => next.add(id));
  return next;
}
