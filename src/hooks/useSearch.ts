import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../lib/api";
import { PageCache, mergeResults, parseBeatmapReference, planFetch, searchKey } from "../lib/results";
import type { FetchRecord } from "../lib/results";
import type { Beatmapset, Mirror, Sort, SortDirection } from "../lib/types";

export interface SearchParams {
  text: string;
  /** 0 osu!, 1 taiko, 2 catch, 3 mania; null for every mode. */
  mode: number | null;
  status: number;
  sort: Sort;
  dir: SortDirection;
}

/** The set a pasted link or id points at, or null when nothing is found (the text is then searched as words). */
async function findByReference(text: string): Promise<Beatmapset | null> {
  const reference = parseBeatmapReference(text);
  if (!reference) return null;
  const asSet = () => api.lookupBeatmapset(reference.id);
  const asBeatmap = async () => api.lookupBeatmapset(await api.resolveSetId(reference.id));
  const readings = reference.kind === "set" ? [asSet] : reference.kind === "beatmap" ? [asBeatmap] : [asSet, asBeatmap];
  for (const reading of readings) {
    try {
      return await reading();
    } catch {
      // This reading found nothing: try the next one, and after the last fall back to an ordinary search.
    }
  }
  return null;
}

const PAGE_SIZE = 30;
/** How long typing must pause before a search is sent: short enough to feel live, long enough to skip keystrokes in a burst. */
const TYPING_DEBOUNCE_MS = 200;

interface State {
  results: Beatmapset[];
  /** How many results the mirror has handed over, which is the next page's offset. */
  fetched: number;
  hasMore: boolean;
  loading: boolean;
  error: string | null;
  /** The offset that failed, so retry resumes there instead of starting over. */
  failedOffset: number;
  mirror: Mirror | null;
  sortApplied: boolean;
}

const EMPTY: State = {
  results: [],
  fetched: 0,
  hasMore: false,
  loading: false,
  error: null,
  failedOffset: 0,
  mirror: null,
  sortApplied: true,
};

/**
 * Live search with paging. Typing is debounced, changing a filter searches at once, and an
 * answer to an older query never replaces the results of a newer one.
 *
 * The first search runs as soon as the app starts, while the overlay is still hidden, so the
 * first time it opens there are results waiting instead of placeholders. After that nothing is
 * requested while `enabled` is false (hidden), and opening it again only refetches results that
 * have gone stale (see `planFetch`).
 */
export function useSearch(params: SearchParams, enabled: boolean) {
  const [state, setState] = useState<State>(EMPTY);
  const generation = useRef(0);
  const latest = useRef(state);
  latest.current = state;
  const previousText = useRef(params.text);
  /** The last query whose first page arrived, and when. */
  const lastFetch = useRef<FetchRecord | null>(null);
  /** The query whose first page is on its way, so opening the overlay doesn't restart it. */
  const inFlight = useRef<string | null>(null);
  /** Recent first pages, so returning to a search already made shows it with no wait. */
  const [pages] = useState(() => new PageCache());
  const key = searchKey({ text: params.text, mode: params.mode, status: params.status, sort: params.sort, dir: params.dir });

  const fetchPage = useCallback(
    async (offset: number, forGeneration: number) => {
      try {
        // A pasted link or id is looked up directly; anything else, or a lookup that finds nothing, is a search.
        const found = offset === 0 ? await findByReference(params.text) : null;
        const response = found
          ? { results: [found], mirror: null, sortApplied: true, hasMore: false }
          : await api.search({
              query: params.text,
              mode: params.mode,
              status: params.status,
              sort: params.sort,
              direction: params.sort === "relevance" ? null : params.dir,
              offset,
              limit: PAGE_SIZE,
            });
        // Kept even when the person has typed on since: backspacing to this search is then instant.
        if (offset === 0) {
          const { results, hasMore, mirror, sortApplied } = response;
          pages.set(key, { results, hasMore, mirror, sortApplied, at: performance.now() });
        }
        if (forGeneration !== generation.current) return;
        if (offset === 0) {
          lastFetch.current = { key, at: performance.now() };
          inFlight.current = null;
        }
        setState((current) => ({
          results: offset === 0 ? response.results : mergeResults(current.results, response.results),
          fetched: offset + response.results.length,
          hasMore: response.hasMore,
          loading: false,
          error: null,
          failedOffset: 0,
          mirror: response.mirror,
          sortApplied: response.sortApplied,
        }));
      } catch (error) {
        if (forGeneration !== generation.current) return;
        if (offset === 0) inFlight.current = null;
        setState((current) => ({ ...current, loading: false, error: String(error), failedOffset: offset }));
      }
    },
    [params.text, params.mode, params.status, params.sort, params.dir, key, pages],
  );

  // A new query, the overlay opening, or the app starting: fetch the first page when it is
  // needed, keeping the old results on screen until the new ones arrive.
  useEffect(() => {
    const typing = params.text !== previousText.current;
    previousText.current = params.text;
    const plan = planFetch({
      enabled,
      typing,
      last: lastFetch.current,
      inFlightKey: inFlight.current,
      key,
      now: performance.now(),
      hadError: latest.current.error !== null,
    });
    if (plan === "skip") return;
    const remembered = pages.get(key, performance.now());
    if (remembered) {
      // Asked before, a moment ago: show it now, and let anything still on its way fall away.
      generation.current += 1;
      inFlight.current = null;
      lastFetch.current = { key, at: remembered.at };
      setState({
        results: remembered.results,
        fetched: remembered.results.length,
        hasMore: remembered.hasMore,
        loading: false,
        error: null,
        failedOffset: 0,
        mirror: remembered.mirror,
        sortApplied: remembered.sortApplied,
      });
      return;
    }
    const forGeneration = ++generation.current;
    inFlight.current = key;
    setState((current) => ({ ...current, loading: true, error: null }));
    let sent = false;
    const timer = setTimeout(
      () => {
        sent = true;
        void fetchPage(0, forGeneration);
      },
      plan === "debounced" ? TYPING_DEBOUNCE_MS : 0,
    );
    return () => {
      clearTimeout(timer);
      // Only a request that was never sent stops being "in flight": opening the overlay re-runs
      // this effect while the first search is still on its way, and that one must be left alone.
      if (!sent && inFlight.current === key && forGeneration === generation.current) inFlight.current = null;
    };
  }, [enabled, fetchPage, key, params.text, pages]);

  const loadMore = useCallback(() => {
    const current = latest.current;
    if (!enabled || current.loading || !current.hasMore || current.error) return;
    setState((s) => ({ ...s, loading: true }));
    void fetchPage(current.fetched, generation.current);
  }, [enabled, fetchPage]);

  const retry = useCallback(() => {
    const forGeneration = ++generation.current;
    setState((s) => ({ ...s, loading: true, error: null }));
    void fetchPage(latest.current.failedOffset, forGeneration);
  }, [fetchPage]);

  return { ...state, loadMore, retry };
}
