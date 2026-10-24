import type { Sort, SortDirection } from "./types";

export interface SortOption {
  value: Sort;
  label: string;
  /** What each direction means for this sort, in plain words; null for relevance, which has none. */
  words: Record<SortDirection, string> | null;
  /** The direction people expect first. */
  first: SortDirection;
}

const NEWEST = { desc: "Newest first", asc: "Oldest first" };
const MOST = { desc: "Most first", asc: "Fewest first" };
const AZ = { asc: "A to Z", desc: "Z to A" };

/** Every order the mirrors can really sort by (checked against osu.direct), in the order the menu lists them. */
export const SORTS: SortOption[] = [
  { value: "relevance", label: "Best match", words: null, first: "desc" },
  { value: "rankedDate", label: "Ranked date", words: NEWEST, first: "desc" },
  { value: "updated", label: "Last updated", words: NEWEST, first: "desc" },
  { value: "submitted", label: "Submitted", words: NEWEST, first: "desc" },
  { value: "plays", label: "Plays", words: MOST, first: "desc" },
  { value: "favourites", label: "Favourites", words: MOST, first: "desc" },
  { value: "difficulty", label: "Stars", words: { desc: "Hardest first", asc: "Easiest first" }, first: "desc" },
  { value: "bpm", label: "BPM", words: { desc: "Fastest first", asc: "Slowest first" }, first: "desc" },
  { value: "length", label: "Length", words: { desc: "Longest first", asc: "Shortest first" }, first: "desc" },
  { value: "title", label: "Title", words: AZ, first: "asc" },
  { value: "artist", label: "Artist", words: AZ, first: "asc" },
];

export const sortOption = (sort: Sort): SortOption => SORTS.find((option) => option.value === sort) ?? SORTS[0];

export const defaultDirection = (sort: Sort): SortDirection => sortOption(sort).first;

/** "Newest first" for the direction the sort is in now, or null when the sort has no direction. */
export const directionWords = (sort: Sort, direction: SortDirection): string | null =>
  sortOption(sort).words?.[direction] ?? null;

export const reverse = (direction: SortDirection): SortDirection => (direction === "asc" ? "desc" : "asc");
