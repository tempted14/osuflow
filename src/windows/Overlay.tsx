import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "../lib/api";
import { DownloadsDrawer } from "../components/DownloadsDrawer";
import { ResultsGrid } from "../components/ResultsGrid";
import { SearchControls } from "../components/SearchControls";
import { useDownloads } from "../hooks/useDownloads";
import { useInstalled } from "../hooks/useInstalled";
import { usePreview } from "../hooks/usePreview";
import { useSearch } from "../hooks/useSearch";
import type { SearchParams } from "../hooks/useSearch";
import { useSettings } from "../hooks/useSettings";
import { setLabel } from "../lib/format";
import { actingIndex, isNavKey, moveSelection } from "../lib/nav";
import { loadPrefs, savePrefs } from "../lib/prefs";
import type { Layout } from "../lib/prefs";
import { parseQuery } from "../lib/query";
import { DEFAULT_STATUSES, FILTER_WANT, NO_FILTERS, activeFilterCount, isDefaultStatuses, keepPaging, narrowsResults, visibleResults } from "../lib/results";
import type { ClientFilters } from "../lib/results";
import type { Beatmapset, OverlayContext } from "../lib/types";

const hide = () => void api.hideOverlay();
/** Hidden this long, the overlay lets go of its cover images: decoded, they are most of its memory. */
const RELEASE_COVERS_AFTER_MS = 15_000;
/** What the grid shows while the overlay is hidden: the results stay fetched, but no cards are kept. */
const NO_SETS: Beatmapset[] = [];
const MODIFIER_KEYS = ["Shift", "Control", "Alt", "Meta"];

/** The card under the pointer right now, by the browser's own :hover, so it is the one drawn highlighted. */
function hoveredCardIndex(): number {
  const card = document.querySelector<HTMLElement>(".card:hover:not(.ghost)");
  const index = card ? Number(card.dataset.index) : -1;
  return Number.isInteger(index) ? index : -1;
}

function tipFor(context: OverlayContext | null): string | null {
  if (!context) return null;
  if (context.exclusiveFullscreen) {
    return "osu! is in exclusive fullscreen, so this overlay can't draw over the game. Switch osu! to borderless or windowed in Options → Graphics.";
  }
  if (!context.osuRunning) return "osu! isn't running. Downloads will open it.";
  return null;
}

/**
 * The osu!direct-style search card that the hotkey opens over osu!. The window is exactly this
 * card (the backend sizes and places it), with Windows' frosted glass behind it.
 */
export default function Overlay() {
  const [visible, setVisible] = useState(false);
  const [context, setContext] = useState<OverlayContext | null>(null);
  // How you like to browse is remembered between runs (what you typed is not).
  const [prefs] = useState(loadPrefs);
  const [params, setParams] = useState<SearchParams>(() => ({
    text: "",
    mode: prefs.mode,
    statuses: prefs.statuses,
    sort: prefs.sort,
    dir: prefs.dir,
  }));
  const [filters, setFilters] = useState<ClientFilters>(prefs.filters);
  const [layout, setLayout] = useState<Layout>(prefs.layout);
  const [panelOpen, setPanelOpen] = useState(prefs.panelOpen);
  const [selected, setSelected] = useState(-1);
  /** The card the pointer is over (for the key hint); Enter itself asks the browser, see `hoveredCardIndex`. */
  const [hover, setHover] = useState(-1);
  /** When the pointer last moved and when a key was last pressed: whichever is later says what the person is doing. */
  const pointerAt = useRef(0);
  const keyAt = useRef(0);
  const [columns, setColumns] = useState(3);
  const inputRef = useRef<HTMLInputElement>(null);

  const { settings, update: updateSettings, reload: reloadSettings } = useSettings();
  const { installed, refresh: refreshInstalled } = useInstalled();
  const { jobs, jobFor, start: startDownload, cancel, dismiss, clearFinished } = useDownloads();
  const { playingId, toggle: togglePreview, stop: stopPreview } = usePreview();
  // Typed conditions like `stars>5` are applied here, on what the mirrors sent; the mirrors only get the
  // rest of the words, so editing a condition never starts a new search.
  const query = useMemo(() => parseQuery(params.text), [params.text]);
  const search = useSearch({ ...params, text: query.text }, visible);

  // The images come straight back from the HTTP cache the next time the overlay is shown. They
  // start out released too: the first results are fetched at start-up, while the overlay is
  // hidden, and loading thirty covers for a window nobody sees would waste the memory saved here.
  const [coversReleased, setCoversReleased] = useState(true);
  useEffect(() => {
    if (visible) {
      setCoversReleased(false);
      return;
    }
    const timer = setTimeout(() => setCoversReleased(true), RELEASE_COVERS_AFTER_MS);
    return () => clearTimeout(timer);
  }, [visible]);

  const keep = useMemo(() => new Set(jobs.map((job) => job.setId)), [jobs]);
  const sets = useMemo(
    () => visibleResults(search.results, installed, settings?.hideInstalled ?? false, keep, filters, params.mode, query),
    [search.results, installed, settings?.hideInstalled, keep, filters, params.mode, query],
  );

  useEffect(() => {
    savePrefs({ mode: params.mode, statuses: params.statuses, sort: params.sort, dir: params.dir, filters, layout, panelOpen });
  }, [params.mode, params.statuses, params.sort, params.dir, filters, layout, panelOpen]);

  const focusSearch = useCallback((selectAll = false) => {
    inputRef.current?.focus();
    if (selectAll) inputRef.current?.select();
  }, []);

  useEffect(() => {
    const listeners = Promise.all([
      listen<OverlayContext>("overlay-shown", ({ payload }) => {
        setContext(payload);
        setVisible(true);
        setSelected(-1);
        // A pointer left resting over a card when the overlay opens is not pointing at it yet.
        keyAt.current = performance.now();
        void refreshInstalled();
        void reloadSettings();
        // The webview may not own keyboard focus yet when the event lands, so try twice.
        focusSearch(true);
        setTimeout(() => focusSearch(true), 80);
      }),
      listen("overlay-hidden", () => {
        setVisible(false);
        stopPreview();
      }),
    ]);
    return () => {
      void listeners.then((stops) => stops.forEach((stop) => stop()));
    };
  }, [refreshInstalled, reloadSettings, focusSearch, stopPreview]);

  useEffect(() => {
    if (selected >= sets.length) setSelected(-1);
  }, [selected, sets.length]);

  const download = useCallback(
    (set: Beatmapset) => {
      // The artist and title name the file, so the backend doesn't have to look them up.
      if (!installed.has(set.id)) startDownload(set.id, setLabel(set), { artist: set.artist, title: set.title });
    },
    [installed, startDownload],
  );

  const changeParams = useCallback((patch: Partial<SearchParams>) => {
    setParams((current) => ({ ...current, ...patch }));
    setSelected(-1);
  }, []);

  const changeFilters = useCallback((patch: Partial<ClientFilters>) => {
    setFilters((current) => ({ ...current, ...patch }));
    setSelected(-1);
  }, []);

  const preview = useCallback((set: Beatmapset) => togglePreview(set.id, set.preview_url), [togglePreview]);

  // The filters run here on what the mirrors sent, so a rare one can leave the grid nearly empty. More pages
  // are fetched for it, but only a few at a time; "Keep searching" asks for a few more.
  const pagesTried = useRef(0);
  const [capped, setCapped] = useState(false);
  // Dragging a slider changes the filters at every step. The grid follows at once, but the allowance for
  // fetching more pages is only refilled when the slider has stopped, so a long drag can't keep asking the mirrors.
  const [settledRanges, setSettledRanges] = useState(filters.ranges);
  useEffect(() => {
    const timer = setTimeout(() => setSettledRanges(filters.ranges), 300);
    return () => clearTimeout(timer);
  }, [filters.ranges]);
  const signature = [
    params.text,
    params.mode,
    params.statuses.join(","),
    params.sort,
    params.dir,
    JSON.stringify({ ...filters, ranges: settledRanges }),
  ].join("|");
  // Counts refills, so one that changes nothing on screen still makes the grid look for more results.
  const [refills, setRefills] = useState(0);
  useEffect(() => {
    pagesTried.current = 0;
    setCapped(false);
    setRefills((count) => count + 1);
  }, [signature]);

  const narrowing = narrowsResults(filters, query);
  const sparse = narrowing && sets.length < FILTER_WANT;
  const { loadMore: fetchMore, loading: fetching } = search;
  const loadMore = useCallback(() => {
    // A page already on its way isn't another one: asking again would only use up the allowance.
    if (fetching) return;
    if (sparse && !keepPaging(narrowing, sets.length, pagesTried.current)) {
      setCapped(true);
      return;
    }
    if (sparse) pagesTried.current += 1;
    fetchMore();
    // `refills` is listed without being read: a new allowance has to make a new callback, which is what
    // wakes the grid's scroll watcher into asking for more.
  }, [fetching, sparse, narrowing, sets.length, fetchMore, refills]);
  const keepSearching = () => {
    pagesTried.current = 0;
    setCapped(false);
    fetchMore();
  };

  const filtersOn = activeFilterCount(filters) > 0 || !isDefaultStatuses(params.statuses) || params.mode !== null;
  const empty = (
    <div className="notice empty">
      <strong>No maps found</strong>
      <span>
        {narrowing ? "Try loosening a filter." : query.text ? "Try fewer or different words." : "Try another category."}
      </span>
      {filtersOn && (
        <button
          onClick={() => {
            changeParams({ statuses: [...DEFAULT_STATUSES], mode: null });
            setFilters(NO_FILTERS);
          }}
        >
          Reset filters
        </button>
      )}
    </div>
  );

  // Follows the pointer, for the key hint and for telling "pointing at a card" from "typing".
  useEffect(() => {
    if (!visible) {
      setHover(-1);
      return;
    }
    const onPointer = (event: PointerEvent) => {
      pointerAt.current = event.timeStamp;
      const card = event.target instanceof Element ? event.target.closest<HTMLElement>(".card:not(.ghost)") : null;
      const index = card ? Number(card.dataset.index) : -1;
      setHover(Number.isInteger(index) ? index : -1);
    };
    const onLeave = () => setHover(-1);
    window.addEventListener("pointermove", onPointer);
    window.addEventListener("pointerdown", onPointer);
    document.documentElement.addEventListener("mouseleave", onLeave);
    return () => {
      window.removeEventListener("pointermove", onPointer);
      window.removeEventListener("pointerdown", onPointer);
      document.documentElement.removeEventListener("mouseleave", onLeave);
    };
  }, [visible]);

  useEffect(() => {
    if (!visible) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        hide();
        return;
      }
      // Was the person pointing, rather than typing, up to this key? Then note that a key was pressed.
      const keyBefore = keyAt.current;
      if (!MODIFIER_KEYS.includes(event.key)) keyAt.current = event.timeStamp;
      const target = event.target;
      // A slider keeps the arrow keys for itself, the way a dropdown does.
      const onSlider = target instanceof HTMLInputElement && target.type === "range";
      const inText = target instanceof HTMLInputElement && target.type !== "checkbox" && !onSlider;
      const inSelect = target instanceof HTMLSelectElement || onSlider;
      const onButton =
        target instanceof HTMLButtonElement || (target instanceof HTMLInputElement && target.type === "checkbox");

      if (isNavKey(event.key)) {
        // Left/Right move the caret in the search box and pick options in a dropdown.
        if (inSelect || (inText && (event.key === "ArrowLeft" || event.key === "ArrowRight"))) return;
        event.preventDefault();
        const next = moveSelection(selected, event.key, columns, sets.length);
        setSelected(next);
        if (next < 0) focusSearch();
        else if (inText) inputRef.current?.blur();
        return;
      }

      // Enter and Space act on the card under a pointer that has moved since the last key (no click needed,
      // whatever has focus), or else on the card chosen with the arrow keys.
      const hovered = event.key === "Enter" || event.key === " " ? hoveredCardIndex() : -1;
      const index = actingIndex({ selected, hovered, pointerAt: pointerAt.current, keyAt: keyBefore });
      const pointing = hovered >= 0 && index === hovered;
      const current = sets[index];
      if (event.key === "Enter" && current && (pointing || (!onButton && !inSelect))) {
        event.preventDefault();
        // Holding the key down must not ask for the same map again and again.
        if (!event.repeat) download(current);
      } else if (event.key === " " && current && !inText && (pointing || (!inSelect && !onButton))) {
        event.preventDefault();
        if (!event.repeat) togglePreview(current.id, current.preview_url);
      } else if (!inText && !inSelect && event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
        // Type-to-search: any printable key sends focus back to the search box.
        focusSearch();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [visible, sets, selected, columns, download, togglePreview, focusSearch]);

  // One quiet line at most: a problem that changes what the overlay can do, never a status report.
  const tip = tipFor(context) ?? (search.sortApplied ? null : "This mirror can't sort, so these are best matches.");

  return (
    <div className={`panel${visible ? " visible" : ""}`} role="dialog" aria-label="osu!flow map search">
      <SearchControls
        inputRef={inputRef}
        params={params}
        query={query}
        settings={settings}
        filters={filters}
        layout={layout}
        panelOpen={panelOpen}
        onChange={changeParams}
        onSettings={updateSettings}
        onFilters={changeFilters}
        onLayout={setLayout}
        onPanel={setPanelOpen}
        onClose={hide}
      />
      <div className={`loadbar${search.loading ? " on" : ""}`} aria-hidden />
      {capped && sparse && search.hasMore ? (
        <p className="hint">
          Few maps match these filters so far.{" "}
          <button className="link" onClick={keepSearching}>
            Keep searching
          </button>
        </p>
      ) : (
        tip && <p className="hint">{tip}</p>
      )}
      <ResultsGrid
        // Prefetched results wait as data. Mounted while hidden, thirty cards would keep layout
        // and compositor layers alive (measured: about 12 MB) for a window nobody can see.
        sets={visible ? sets : NO_SETS}
        installed={installed}
        jobFor={jobFor}
        selected={selected}
        playingId={playingId}
        hasMore={search.hasMore}
        loading={search.loading}
        error={search.error}
        releaseCovers={coversReleased}
        layout={layout}
        onColumnsChange={setColumns}
        empty={empty}
        onLoadMore={loadMore}
        onRetry={search.retry}
        onSelect={setSelected}
        onDownload={download}
        onCancel={cancel}
        onPreview={preview}
      />
      {/* Floats over the results, so showing it never moves anything. */}
      <div className={`keyhint${visible && (selected >= 0 || hover >= 0) ? " on" : ""}`} aria-hidden>
        <kbd>Enter</kbd> download
        <i />
        <kbd>Space</kbd> preview
      </div>
      <DownloadsDrawer
        jobs={jobs}
        onCancel={cancel}
        onRetry={(job) => startDownload(job.setId, job.label)}
        onDismiss={dismiss}
        onClear={clearFinished}
        onOpenPage={(setId) => void api.openBeatmapsetPage(setId)}
      />
    </div>
  );
}
