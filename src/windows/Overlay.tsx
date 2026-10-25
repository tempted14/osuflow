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
import { isNavKey, moveSelection } from "../lib/nav";
import { loadPrefs, savePrefs } from "../lib/prefs";
import type { Layout } from "../lib/prefs";
import { FILTER_WANT, NO_FILTERS, activeFilterCount, keepPaging, narrowsResults, visibleResults } from "../lib/results";
import type { ClientFilters } from "../lib/results";
import type { Beatmapset, OverlayContext } from "../lib/types";

const hide = () => void api.hideOverlay();
/** Hidden this long, the overlay lets go of its cover images: decoded, they are most of its memory. */
const RELEASE_COVERS_AFTER_MS = 15_000;
/** What the grid shows while the overlay is hidden: the results stay fetched, but no cards are kept. */
const NO_SETS: Beatmapset[] = [];

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
    status: prefs.status,
    sort: prefs.sort,
    dir: prefs.dir,
  }));
  const [filters, setFilters] = useState<ClientFilters>(prefs.filters);
  const [layout, setLayout] = useState<Layout>(prefs.layout);
  const [panelOpen, setPanelOpen] = useState(prefs.panelOpen);
  const [selected, setSelected] = useState(-1);
  const [columns, setColumns] = useState(3);
  const inputRef = useRef<HTMLInputElement>(null);

  const { settings, update: updateSettings, reload: reloadSettings } = useSettings();
  const { installed, refresh: refreshInstalled } = useInstalled();
  const { jobs, jobFor, start: startDownload, cancel, dismiss, clearFinished } = useDownloads();
  const { playingId, toggle: togglePreview, stop: stopPreview } = usePreview();
  const search = useSearch(params, visible);

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
    () => visibleResults(search.results, installed, settings?.hideInstalled ?? false, keep, filters, params.mode),
    [search.results, installed, settings?.hideInstalled, keep, filters, params.mode],
  );

  useEffect(() => {
    savePrefs({ mode: params.mode, status: params.status, sort: params.sort, dir: params.dir, filters, layout, panelOpen });
  }, [params.mode, params.status, params.sort, params.dir, filters, layout, panelOpen]);

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
  const signature = [params.text, params.mode, params.status, params.sort, params.dir, JSON.stringify(filters)].join("|");
  useEffect(() => {
    pagesTried.current = 0;
    setCapped(false);
  }, [signature]);

  const narrowing = narrowsResults(filters);
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
  }, [fetching, sparse, narrowing, sets.length, fetchMore]);
  const keepSearching = () => {
    pagesTried.current = 0;
    setCapped(false);
    fetchMore();
  };

  const filtersOn = activeFilterCount(filters) > 0 || params.status !== 1 || params.mode !== null;
  const empty = (
    <div className="notice empty">
      <strong>No maps found</strong>
      <span>
        {narrowing ? "Try loosening a filter." : params.text ? "Try fewer or different words." : "Try another category."}
      </span>
      {filtersOn && (
        <button
          onClick={() => {
            changeParams({ status: 1, mode: null });
            setFilters(NO_FILTERS);
          }}
        >
          Reset filters
        </button>
      )}
    </div>
  );

  useEffect(() => {
    if (!visible) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        hide();
        return;
      }
      const target = event.target;
      const inText = target instanceof HTMLInputElement && target.type !== "checkbox";
      const inSelect = target instanceof HTMLSelectElement;
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

      const current = sets[selected];
      if (event.key === "Enter" && current && !onButton && !inSelect) {
        event.preventDefault();
        download(current);
      } else if (event.key === " " && current && !inText && !inSelect && !onButton) {
        event.preventDefault();
        togglePreview(current.id, current.preview_url);
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
      {selected >= 0 && <p className="hint keys">Enter to download · Space to preview</p>}
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
