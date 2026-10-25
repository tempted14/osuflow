import type { Beatmapset, LibraryDelta } from "./types";

/** Appends a page of results, skipping sets that are already listed (mirrors can repeat them). */
export function mergeResults(existing: Beatmapset[], incoming: Beatmapset[]): Beatmapset[] {
  const seen = new Set(existing.map((set) => set.id));
  return [...existing, ...incoming.filter((set) => !seen.has(set.id))];
}

/** The bands osu! colours by star rating: a way to ask for maps of a level without typing numbers. */
export type Tier = "any" | "easy" | "normal" | "hard" | "insane" | "expert" | "expertPlus";

export const TIERS: { value: Tier; label: string; short: string; min: number; max: number }[] = [
  { value: "any", label: "Any difficulty", short: "Any", min: 0, max: Infinity },
  { value: "easy", label: "Easy · under 2★", short: "Easy", min: 0, max: 2 },
  { value: "normal", label: "Normal · 2–2.7★", short: "Normal", min: 2, max: 2.7 },
  { value: "hard", label: "Hard · 2.7–4★", short: "Hard", min: 2.7, max: 4 },
  { value: "insane", label: "Insane · 4–5.3★", short: "Insane", min: 4, max: 5.3 },
  { value: "expert", label: "Expert · 5.3–6.5★", short: "Expert", min: 5.3, max: 6.5 },
  { value: "expertPlus", label: "Expert+ · 6.5★ and up", short: "Expert+", min: 6.5, max: Infinity },
];

/** The categories a search can ask the mirrors for, as osu! API status ids (the mirrors accept -2 to 4). */
export const STATUSES: [string, number][] = [
  ["Ranked", 1],
  ["Loved", 4],
  ["Qualified", 3],
  ["Pending", 0],
  ["WIP", -1],
  ["Graveyard", -2],
];

/** osu!'s own genre ids, in the order its website lists them (checked against real mirror results). */
export const GENRES: { id: number; label: string }[] = [
  { id: 1, label: "Unspecified" },
  { id: 2, label: "Video Game" },
  { id: 3, label: "Anime" },
  { id: 4, label: "Rock" },
  { id: 5, label: "Pop" },
  { id: 6, label: "Other" },
  { id: 7, label: "Novelty" },
  { id: 9, label: "Hip Hop" },
  { id: 10, label: "Electronic" },
  { id: 11, label: "Metal" },
  { id: 12, label: "Classical" },
  { id: 13, label: "Folk" },
  { id: 14, label: "Jazz" },
];

/** osu!'s own language ids, in the order its website lists them. */
export const LANGUAGES: { id: number; label: string }[] = [
  { id: 2, label: "English" },
  { id: 4, label: "Chinese" },
  { id: 7, label: "French" },
  { id: 8, label: "German" },
  { id: 11, label: "Italian" },
  { id: 3, label: "Japanese" },
  { id: 6, label: "Korean" },
  { id: 10, label: "Spanish" },
  { id: 9, label: "Swedish" },
  { id: 12, label: "Russian" },
  { id: 13, label: "Polish" },
  { id: 5, label: "Instrumental" },
  { id: 14, label: "Other" },
  { id: 1, label: "Unspecified" },
];

/** Everything that narrows the results inside the app, because neither mirror can filter by any of it. */
export interface ClientFilters {
  tier: Tier;
  /** osu!'s genre id, or null for any. */
  genre: number | null;
  /** osu!'s language id, or null for any. */
  language: number | null;
  video: boolean;
  storyboard: boolean;
  /** Only Featured Artist songs. */
  featured: boolean;
  /** Only spotlighted maps. */
  spotlight: boolean;
  /** Explicit maps stay hidden unless this is on, as on the osu! website. */
  showExplicit: boolean;
}

export const NO_FILTERS: ClientFilters = {
  tier: "any",
  genre: null,
  language: null,
  video: false,
  storyboard: false,
  featured: false,
  spotlight: false,
  showExplicit: false,
};

/** How many settings differ from the defaults: the number on the Filters button. */
export function activeFilterCount(filters: ClientFilters): number {
  return (Object.keys(NO_FILTERS) as (keyof ClientFilters)[]).filter((key) => filters[key] !== NO_FILTERS[key]).length;
}

/** True when a filter can leave few matches on a page. Showing explicit maps doesn't narrow anything. */
export function narrowsResults(filters: ClientFilters): boolean {
  return activeFilterCount({ ...filters, showExplicit: NO_FILTERS.showExplicit }) > 0;
}

/** True when the set has a difficulty in the tier (counting only the chosen mode's, if a mode is chosen). */
export function hasDifficultyIn(set: Pick<Beatmapset, "beatmaps">, tier: Tier, mode: number | null): boolean {
  const range = TIERS.find((candidate) => candidate.value === tier);
  if (!range || tier === "any") return true;
  return set.beatmaps.some(
    (map) =>
      (mode === null || map.mode_int === mode) && map.difficulty_rating >= range.min && map.difficulty_rating < range.max,
  );
}

/** Whether one set passes every filter. A set the mirror gave no genre or language for fails those two filters. */
export function matchesFilters(set: Beatmapset, filters: ClientFilters, mode: number | null): boolean {
  if (!filters.showExplicit && set.nsfw) return false;
  if (filters.genre !== null && set.genre_id !== filters.genre) return false;
  if (filters.language !== null && set.language_id !== filters.language) return false;
  if (filters.video && !set.video) return false;
  if (filters.storyboard && !set.storyboard) return false;
  if (filters.featured && !set.track_id) return false;
  if (filters.spotlight && !set.spotlight) return false;
  return hasDifficultyIn(set, filters.tier, mode);
}

/**
 * What the grid shows. With "hide installed" on, sets already in the library disappear, except
 * ones with a download in progress or just finished, so a card doesn't vanish under the cursor.
 * The filters (difficulty, genre, language, extras, explicit content) run here on what the
 * mirrors sent, which is instant and needs no new search.
 */
export function visibleResults(
  results: Beatmapset[],
  installed: ReadonlySet<number>,
  hideInstalled: boolean,
  keep: ReadonlySet<number>,
  filters: ClientFilters = NO_FILTERS,
  mode: number | null = null,
): Beatmapset[] {
  return results.filter(
    (set) => matchesFilters(set, filters, mode) && (!hideInstalled || !installed.has(set.id) || keep.has(set.id)),
  );
}

/** With a filter on, how many matches are enough to stop fetching pages by itself. */
export const FILTER_WANT = 10;
/** And how many pages it may fetch in one go looking for them, so a rare filter can't hammer the mirrors. */
export const FILTER_AUTO_PAGES = 6;

/** Whether to fetch another page unasked: only with a filter on, only while few maps match, and only so far. */
export function keepPaging(narrowing: boolean, shown: number, pagesTried: number): boolean {
  return narrowing && shown < FILTER_WANT && pagesTried < FILTER_AUTO_PAGES;
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
