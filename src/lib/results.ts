import type { Beatmapset, LibraryDelta } from "./types";

/** Appends a page of results, skipping sets that are already listed (mirrors can repeat them). */
export function mergeResults(existing: Beatmapset[], incoming: Beatmapset[]): Beatmapset[] {
  const seen = new Set(existing.map((set) => set.id));
  return [...existing, ...incoming.filter((set) => !seen.has(set.id))];
}

/** The bands osu! colours by star rating: a way to ask for maps of a level without typing numbers. */
export type Tier = "any" | "easy" | "normal" | "hard" | "insane" | "expert" | "expertPlus";

export const TIERS: { value: Tier; label: string; min: number; max: number }[] = [
  { value: "any", label: "Any", min: 0, max: Infinity },
  { value: "easy", label: "Easy · under 2★", min: 0, max: 2 },
  { value: "normal", label: "Normal · 2–2.7★", min: 2, max: 2.7 },
  { value: "hard", label: "Hard · 2.7–4★", min: 2.7, max: 4 },
  { value: "insane", label: "Insane · 4–5.3★", min: 4, max: 5.3 },
  { value: "expert", label: "Expert · 5.3–6.5★", min: 5.3, max: 6.5 },
  { value: "expertPlus", label: "Expert+ · 6.5★ up", min: 6.5, max: Infinity },
];

/** True when the set has a difficulty in the tier (counting only the chosen mode's, if a mode is chosen). */
export function hasDifficultyIn(set: Pick<Beatmapset, "beatmaps">, tier: Tier, mode: number | null): boolean {
  const range = TIERS.find((candidate) => candidate.value === tier);
  if (!range || tier === "any") return true;
  return set.beatmaps.some(
    (map) =>
      (mode === null || map.mode_int === mode) && map.difficulty_rating >= range.min && map.difficulty_rating < range.max,
  );
}

/**
 * What the grid shows. With "hide installed" on, sets already in the library disappear, except
 * ones with a download in progress or just finished, so a card doesn't vanish under the cursor.
 * A difficulty tier keeps only sets that have a difficulty in it. The mirrors can't filter by
 * stars themselves, so this runs here on what they sent.
 */
export function visibleResults(
  results: Beatmapset[],
  installed: ReadonlySet<number>,
  hideInstalled: boolean,
  keep: ReadonlySet<number>,
  tier: Tier = "any",
  mode: number | null = null,
): Beatmapset[] {
  if (!hideInstalled && tier === "any") return results;
  return results.filter(
    (set) => hasDifficultyIn(set, tier, mode) && (!hideInstalled || !installed.has(set.id) || keep.has(set.id)),
  );
}

/** With a difficulty tier on, how many matches are enough to stop fetching pages by itself. */
export const FILTER_WANT = 10;
/** And how many pages it may fetch in one go looking for them, so a rare tier can't hammer the mirrors. */
export const FILTER_AUTO_PAGES = 6;

/** Whether to fetch another page unasked: only with a tier on, only while few maps match, and only so far. */
export function keepPaging(tier: Tier, shown: number, pagesTried: number): boolean {
  return tier !== "any" && shown < FILTER_WANT && pagesTried < FILTER_AUTO_PAGES;
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
export function searchKey(params: {
  text: string;
  mode: number | null;
  status: number;
  sort: string;
  dir?: string;
}): string {
  // Relevance has no direction, so flipping it must not count as a new search.
  const dir = params.sort === "relevance" ? null : (params.dir ?? null);
  return JSON.stringify([params.text.trim().toLowerCase(), params.mode, params.status, params.sort, dir]);
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

/** A beatmap link or id typed into the search box. "either" is a bare number: a set id or a beatmap id. */
export interface BeatmapReference {
  kind: "set" | "beatmap" | "either";
  id: number;
}

/**
 * Recognises an osu! beatmap link (the website's `/beatmapsets/`, `/s/`, `/beatmaps/` and `/b/` forms)
 * or a bare id, so pasting one finds that map directly. Short numbers stay ordinary searches: a song
 * called "1999" shouldn't turn into a lookup of set 1999.
 */
export function parseBeatmapReference(text: string): BeatmapReference | null {
  const trimmed = text.trim();
  const link = /^https?:\/\/(?:osu|old)\.ppy\.sh\/(beatmapsets|s|beatmaps|b)\/(\d{1,9})(?!\d)/i.exec(trimmed);
  if (link) {
    const kind = ["beatmapsets", "s"].includes(link[1].toLowerCase()) ? "set" : "beatmap";
    return { kind, id: Number(link[2]) };
  }
  if (/^\d{5,9}$/.test(trimmed)) return { kind: "either", id: Number(trimmed) };
  return null;
}

export function applyLibraryDelta(current: ReadonlySet<number>, delta: LibraryDelta): ReadonlySet<number> {
  const next = new Set(current);
  delta.removed.forEach((id) => next.delete(id));
  delta.added.forEach((id) => next.add(id));
  return next;
}
