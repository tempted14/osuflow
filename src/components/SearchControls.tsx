import type { RefObject } from "react";
import type { SearchParams } from "../hooks/useSearch";
import type { Settings, Sort } from "../lib/types";

const MODES: [string, number | null][] = [
  ["All", null],
  ["osu!", 0],
  ["taiko", 1],
  ["catch", 2],
  ["mania", 3],
];

const STATUSES: [string, number][] = [
  ["Ranked", 1],
  ["Loved", 4],
  ["Qualified", 3],
  ["Pending", 0],
  ["Graveyard", -2],
];

const SORTS: [string, Sort][] = [
  ["Best match", "relevance"],
  ["Newest ranked", "rankedDate"],
  ["Most played", "plays"],
  ["Most favourited", "favourites"],
  ["Recently updated", "updated"],
  ["Hardest", "difficulty"],
  ["Title A–Z", "title"],
  ["Artist A–Z", "artist"],
];

function SearchIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden>
      <circle cx="11" cy="11" r="7" />
      <path d="m20 20-3.5-3.5" />
    </svg>
  );
}

/** The search box and one quiet row of filters. "Esc" doubles as the close button. */
export function SearchControls(props: {
  inputRef: RefObject<HTMLInputElement | null>;
  params: SearchParams;
  settings: Settings | null;
  onChange: (patch: Partial<SearchParams>) => void;
  onSettings: (patch: Partial<Settings>) => void;
  onClose: () => void;
}) {
  const { params, settings } = props;
  return (
    <header className="controls">
      <div className="search-field">
        <SearchIcon />
        <input
          ref={props.inputRef}
          className="search"
          type="search"
          placeholder="Search maps"
          value={params.text}
          onChange={(e) => props.onChange({ text: e.currentTarget.value })}
          spellCheck={false}
          autoComplete="off"
        />
        <button className="esc" onClick={props.onClose} aria-label="Close" title="Close (Esc)">
          esc
        </button>
      </div>
      <div className="filters">
        <div className="tabs" role="group" aria-label="Game mode">
          {MODES.map(([label, mode]) => (
            <button
              key={label}
              className={params.mode === mode ? "on" : ""}
              aria-pressed={params.mode === mode}
              onClick={() => props.onChange({ mode })}
            >
              {label}
            </button>
          ))}
        </div>
        <select
          aria-label="Status"
          value={params.status}
          onChange={(e) => props.onChange({ status: Number(e.currentTarget.value) })}
        >
          {STATUSES.map(([label, value]) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
        <select
          aria-label="Sort by"
          value={params.sort}
          onChange={(e) => props.onChange({ sort: e.currentTarget.value as Sort })}
        >
          {SORTS.map(([label, value]) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
        <label className="toggle">
          Hide installed
          <input
            type="checkbox"
            checked={settings?.hideInstalled ?? false}
            disabled={!settings}
            onChange={(e) => props.onSettings({ hideInstalled: e.currentTarget.checked })}
          />
        </label>
      </div>
    </header>
  );
}
