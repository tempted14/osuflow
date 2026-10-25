import { GENRES, LANGUAGES, NO_FILTERS, STATUSES, TIERS } from "./results";
import type { ClientFilters, Tier } from "./results";
import { SORTS, defaultDirection } from "./sorting";
import type { Sort, SortDirection } from "./types";

export type Layout = "cards" | "list";

/** How the overlay remembers you like to browse between runs. What you typed is never kept. */
export interface Prefs {
  mode: number | null;
  status: number;
  sort: Sort;
  dir: SortDirection;
  filters: ClientFilters;
  layout: Layout;
  /** Whether the filter panel is open. */
  panelOpen: boolean;
}

const KEY = "osuflow.browse.v1";

function oneOf<T>(value: unknown, allowed: readonly T[]): value is T {
  return allowed.includes(value as T);
}

/**
 * Reads saved preferences, trusting nothing: anything missing, unknown or of the wrong type falls back to its
 * default on its own, so a half-written or hand-edited value can't break the overlay or discard the rest.
 */
export function parsePrefs(raw: string | null): Prefs {
  let data: Record<string, unknown> = {};
  try {
    const parsed: unknown = raw ? JSON.parse(raw) : {};
    if (parsed && typeof parsed === "object") data = parsed as Record<string, unknown>;
  } catch {
    // Unreadable: start from the defaults.
  }
  const saved = data.filters && typeof data.filters === "object" ? (data.filters as Record<string, unknown>) : {};
  const flag = (value: unknown, fallback: boolean) => (typeof value === "boolean" ? value : fallback);
  const idIn = (value: unknown, list: { id: number }[]) =>
    typeof value === "number" && list.some((item) => item.id === value) ? value : null;
  const sort = oneOf<Sort>(data.sort, SORTS.map((option) => option.value)) ? data.sort : "relevance";
  return {
    mode: oneOf<number>(data.mode, [0, 1, 2, 3]) ? data.mode : null,
    status: oneOf<number>(data.status, STATUSES.map(([, value]) => value)) ? data.status : 1,
    sort,
    dir: data.dir === "asc" || data.dir === "desc" ? data.dir : defaultDirection(sort),
    filters: {
      tier: oneOf<Tier>(saved.tier, TIERS.map((tier) => tier.value)) ? saved.tier : NO_FILTERS.tier,
      genre: idIn(saved.genre, GENRES),
      language: idIn(saved.language, LANGUAGES),
      video: flag(saved.video, NO_FILTERS.video),
      storyboard: flag(saved.storyboard, NO_FILTERS.storyboard),
      featured: flag(saved.featured, NO_FILTERS.featured),
      spotlight: flag(saved.spotlight, NO_FILTERS.spotlight),
      showExplicit: flag(saved.showExplicit, NO_FILTERS.showExplicit),
    },
    layout: data.layout === "cards" || data.layout === "list" ? data.layout : "list",
    panelOpen: flag(data.panelOpen, false),
  };
}

export function loadPrefs(): Prefs {
  try {
    return parsePrefs(window.localStorage.getItem(KEY));
  } catch {
    return parsePrefs(null); // storage can be blocked or cleared: browse with the defaults
  }
}

export function savePrefs(prefs: Prefs): void {
  try {
    window.localStorage.setItem(KEY, JSON.stringify(prefs));
  } catch {
    // Storage can be blocked: the choices simply don't last until next time.
  }
}
