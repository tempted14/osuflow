import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../lib/api";
import { mergeResults, planFetch, searchKey } from "../lib/results";
import type { FetchRecord } from "../lib/results";
import type { Beatmapset, Mirror, Sort } from "../lib/types";

export interface SearchParams {
  text: string;
  /** 0 osu!, 1 taiko, 2 catch, 3 mania; null for every mode. */
  mode: number | null;
  status: number;
  sort: Sort;
}

const PAGE_SIZE = 30;
const TYPING_DEBOUNCE_MS = 300;

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
  const key = searchKey({ text: params.text, mode: params.mode, status: params.status, sort: params.sort });

  const fetchPage = useCallback(
    async (offset: number, forGeneration: number) => {
      try {
        const response = await api.search({
          query: params.text,
          mode: params.mode,
          status: params.status,
          sort: params.sort,
          offset,
          limit: PAGE_SIZE,
        });
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
    [params.text, params.mode, params.status, params.sort, key],
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
  }, [enabled, fetchPage, key, params.text]);

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
