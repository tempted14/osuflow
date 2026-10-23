import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../lib/api";
import { mergeResults } from "../lib/results";
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
 * answer to an older query never replaces the results of a newer one. Nothing is requested while
 * `enabled` is false (the overlay is hidden).
 */
export function useSearch(params: SearchParams, enabled: boolean) {
  const [state, setState] = useState<State>(EMPTY);
  const generation = useRef(0);
  const latest = useRef(state);
  latest.current = state;
  const previousText = useRef(params.text);

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
        setState((current) => ({ ...current, loading: false, error: String(error), failedOffset: offset }));
      }
    },
    [params.text, params.mode, params.status, params.sort],
  );

  // A new query (or the overlay opening): fetch the first page, keeping the old results on
  // screen until the new ones arrive.
  useEffect(() => {
    if (!enabled) return;
    const forGeneration = ++generation.current;
    const typing = params.text !== previousText.current;
    previousText.current = params.text;
    setState((current) => ({ ...current, loading: true, error: null }));
    const timer = setTimeout(() => void fetchPage(0, forGeneration), typing ? TYPING_DEBOUNCE_MS : 0);
    return () => clearTimeout(timer);
  }, [enabled, fetchPage, params.text]);

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
