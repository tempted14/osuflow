import type { RefObject } from "react";
import type { SearchParams } from "../hooks/useSearch";
import { TIERS } from "../lib/results";
import type { Tier } from "../lib/results";
import { SORTS, defaultDirection, directionWords, reverse } from "../lib/sorting";
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

function SearchIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden>
      <circle cx="11" cy="11" r="7" />
      <path d="m20 20-3.5-3.5" />
    </svg>
  );
}

function ClearIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" aria-hidden>
      <path d="m6 6 12 12M18 6 6 18" />
    </svg>
  );
}

/** Points down for a descending order and up for an ascending one. */
function ArrowIcon({ down }: { down: boolean }) {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.4"
      strokeLinecap="round"
      strokeLinejoin="round"
      style={down ? undefined : { transform: "rotate(180deg)" }}
      aria-hidden
    >
      <path d="M12 5v14m0 0-6-6m6 6 6-6" />
    </svg>
  );
}

/**
 * The search box and one quiet row of filters, each named so nothing has to be guessed. "Esc" doubles
 * as the close button, and the order has a plain-words button ("Newest first") that reverses it.
 */
export function SearchControls(props: {
  inputRef: RefObject<HTMLInputElement | null>;
  params: SearchParams;
  settings: Settings | null;
  onChange: (patch: Partial<SearchParams>) => void;
  onSettings: (patch: Partial<Settings>) => void;
  onClose: () => void;
}) {
  const { params, settings } = props;
  const words = directionWords(params.sort, params.dir);
  return (
    <header className="controls">
      <div className="search-field">
        <SearchIcon />
        <input
          ref={props.inputRef}
          className="search"
          type="search"
          placeholder="Search by title, artist or mapper, or paste a link"
          value={params.text}
          onChange={(e) => props.onChange({ text: e.currentTarget.value })}
          spellCheck={false}
          autoComplete="off"
        />
        {params.text && (
          <button
            className="clear"
            onClick={() => {
              props.onChange({ text: "" });
              props.inputRef.current?.focus();
            }}
            aria-label="Clear the search"
            title="Clear"
          >
            <ClearIcon />
          </button>
        )}
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
        <label className="field">
          <span>Status</span>
          <select value={params.status} onChange={(e) => props.onChange({ status: Number(e.currentTarget.value) })}>
            {STATUSES.map(([label, value]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>Sort</span>
          <select
            value={params.sort}
            onChange={(e) => {
              const sort = e.currentTarget.value as Sort;
              props.onChange({ sort, dir: defaultDirection(sort) });
            }}
          >
            {SORTS.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </label>
        {words && (
          <button
            className="dir"
            onClick={() => props.onChange({ dir: reverse(params.dir) })}
            title="Reverse the order"
            aria-label={`${words}. Reverse the order`}
          >
            <ArrowIcon down={params.dir === "desc"} />
            {words}
          </button>
        )}
        <label className="field">
          <span>Stars</span>
          <select value={params.tier} onChange={(e) => props.onChange({ tier: e.currentTarget.value as Tier })}>
            {TIERS.map((tier) => (
              <option key={tier.value} value={tier.value}>
                {tier.label}
              </option>
            ))}
          </select>
        </label>
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
