import type { ReactNode, RefObject } from "react";
import type { SearchParams } from "../hooks/useSearch";
import type { Layout } from "../lib/prefs";
import { withoutChip } from "../lib/query";
import type { ParsedQuery } from "../lib/query";
import { STATUSES, activeFilterCount } from "../lib/results";
import type { ClientFilters } from "../lib/results";
import { SORTS, defaultDirection, directionWords, reverse } from "../lib/sorting";
import type { Settings, Sort } from "../lib/types";
import { FilterPanel } from "./FilterPanel";
import { Menu } from "./Menu";
import { Segmented } from "./Segmented";

const MODES: { value: number | null; label: string }[] = [
  { value: null, label: "All" },
  { value: 0, label: "osu!" },
  { value: 1, label: "taiko" },
  { value: 2, label: "catch" },
  { value: 3, label: "mania" },
];

const STATUS_OPTIONS = STATUSES.map(([label, value]) => ({ value, label }));
const SORT_OPTIONS = SORTS.map((option) => ({ value: option.value, label: option.label }));

function Icon(props: { children: ReactNode; size?: number; rotate?: number }) {
  const size = props.size ?? 14;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.2"
      strokeLinecap="round"
      strokeLinejoin="round"
      style={props.rotate ? { transform: `rotate(${props.rotate}deg)` } : undefined}
      aria-hidden
    >
      {props.children}
    </svg>
  );
}

/**
 * The search box and the controls that matter every time: game mode, category, sort and its direction,
 * the layout, and one button for everything else. "Esc" doubles as the close button.
 */
export function SearchControls(props: {
  inputRef: RefObject<HTMLInputElement | null>;
  params: SearchParams;
  /** What the typed text was understood as: its conditions are shown under the box. */
  query: ParsedQuery;
  settings: Settings | null;
  filters: ClientFilters;
  layout: Layout;
  panelOpen: boolean;
  onChange: (patch: Partial<SearchParams>) => void;
  onSettings: (patch: Partial<Settings>) => void;
  onFilters: (patch: Partial<ClientFilters>) => void;
  onLayout: (layout: Layout) => void;
  onPanel: (open: boolean) => void;
  onClose: () => void;
}) {
  const { params, settings, filters } = props;
  const words = directionWords(params.sort, params.dir);
  const active = activeFilterCount(filters) + (settings?.hideInstalled ? 1 : 0);
  return (
    <header className="controls">
      <div className="search-field">
        <Icon size={18}>
          <circle cx="11" cy="11" r="7" />
          <path d="m20 20-3.5-3.5" />
        </Icon>
        <input
          ref={props.inputRef}
          className="search"
          type="search"
          placeholder="Search by title, artist or mapper, or paste a link"
          aria-label="Search beatmaps"
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
            <Icon size={12}>
              <path d="m6 6 12 12M18 6 6 18" />
            </Icon>
          </button>
        )}
        <button className="esc" onClick={props.onClose} aria-label="Close" title="Close (Esc)">
          esc
        </button>
      </div>
      {(props.query.chips.length > 0 || props.query.ignored.length > 0) && (
        <div className="conditions" role="group" aria-label="Search conditions">
          {props.query.chips.map((chip) => (
            <button
              key={`${chip.start}:${chip.label}`}
              className="cond"
              title="Remove this condition"
              aria-label={`Remove ${chip.label}`}
              onClick={() => {
                props.onChange({ text: withoutChip(params.text, chip) });
                props.inputRef.current?.focus();
              }}
            >
              {chip.label}
              <Icon size={10}>
                <path d="m6 6 12 12M18 6 6 18" />
              </Icon>
            </button>
          ))}
          {props.query.ignored.map((word) => (
            <span key={word} className="cond-note">
              Can't read {word}
            </span>
          ))}
        </div>
      )}
      <div className="toolbar">
        <Segmented label="Game mode" options={MODES} value={params.mode} onChange={(mode) => props.onChange({ mode })} />
        <i className="vsep" aria-hidden />
        <Menu label="Show" options={STATUS_OPTIONS} value={params.status} onChange={(status) => props.onChange({ status })} />
        <Menu
          label="Sort"
          options={SORT_OPTIONS}
          value={params.sort}
          onChange={(sort: Sort) => props.onChange({ sort, dir: defaultDirection(sort) })}
        />
        {words && (
          <button
            className="dir"
            onClick={() => props.onChange({ dir: reverse(params.dir) })}
            title="Reverse the order"
            aria-label={`${words}. Reverse the order`}
          >
            <Icon size={13} rotate={params.dir === "desc" ? 0 : 180}>
              <path d="M12 5v14m0 0-6-6m6 6 6-6" />
            </Icon>
            {words}
          </button>
        )}
        <div className="toolbar-end">
          <div className="layout" role="group" aria-label="Layout">
            <button
              className={props.layout === "list" ? "on" : ""}
              aria-pressed={props.layout === "list"}
              onClick={() => props.onLayout("list")}
              title="List"
              aria-label="List layout"
            >
              <Icon size={15}>
                <path d="M4 7h16M4 12h16M4 17h16" />
              </Icon>
            </button>
            <button
              className={props.layout === "cards" ? "on" : ""}
              aria-pressed={props.layout === "cards"}
              onClick={() => props.onLayout("cards")}
              title="Cards"
              aria-label="Card layout"
            >
              <Icon size={15}>
                <path d="M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z" />
              </Icon>
            </button>
          </div>
          <button
            className={`filters-btn${props.panelOpen ? " on" : ""}`}
            aria-expanded={props.panelOpen}
            onClick={() => props.onPanel(!props.panelOpen)}
            title="Genre, language, difficulty and more"
          >
            <Icon size={14}>
              <path d="M4 6h16M7 12h10M10 18h4" />
            </Icon>
            Filters
            {active > 0 && <span className="count">{active}</span>}
            <Icon size={12} rotate={props.panelOpen ? 180 : 0}>
              <path d="m6 9 6 6 6-6" />
            </Icon>
          </button>
        </div>
      </div>
      <FilterPanel
        open={props.panelOpen}
        filters={filters}
        settings={settings}
        onFilters={props.onFilters}
        onSettings={props.onSettings}
      />
    </header>
  );
}
