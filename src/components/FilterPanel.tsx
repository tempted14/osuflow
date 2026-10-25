import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { SLIDERS, spanActive, spanShort } from "../lib/ranges";
import type { SliderDef, SliderKey, Span } from "../lib/ranges";
import { GENRES, LANGUAGES, NO_FILTERS, TIERS, activeFilterCount } from "../lib/results";
import type { ClientFilters } from "../lib/results";
import type { Settings } from "../lib/types";
import { RangeSlider } from "./RangeSlider";

/** How long a slider takes to fade away on closing. It keeps its place until then, so the others don't jump. */
const LEAVE_MS = 160;

function Chip(props: { on: boolean; onClick: () => void; title?: string; children: ReactNode }) {
  return (
    <button className={`chip${props.on ? " on" : ""}`} aria-pressed={props.on} title={props.title} onClick={props.onClick}>
      {props.children}
    </button>
  );
}

/** An option that opens a slider rather than choosing a value: blue, with an arrow, and the setting shown on it. */
function RangeChip(props: { def: SliderDef; span: Span; open: boolean; onClick: () => void }) {
  const value = spanShort(props.def, props.span);
  return (
    <button
      className={`chip range${props.open ? " open" : ""}${value ? " set" : ""}`}
      aria-expanded={props.open}
      title={`${props.def.hint}: ${props.open ? "hide" : "show"} the slider`}
      onClick={props.onClick}
    >
      {props.def.label}
      {value && <span className="chip-value">{value}</span>}
      <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
        <path d="m6 9 6 6 6-6" />
      </svg>
    </button>
  );
}

function Row(props: { label: string; children: ReactNode }) {
  return (
    <div className="frow">
      <span className="flabel">{props.label}</span>
      <div className="fchips">{props.children}</div>
    </div>
  );
}

const plus = <K,>(set: ReadonlySet<K>, key: K) => new Set(set).add(key);
const minus = <K,>(set: ReadonlySet<K>, key: K) => {
  const next = new Set(set);
  next.delete(key);
  return next;
};

/**
 * The osu! website's filter list, in its order and its words, made compact. What needs an osu! account (rank
 * achieved, played, favourites, subscribed mappers) can't exist here. Every filter works on the results already
 * fetched, so choosing one is instant and never starts a new search.
 *
 * The blue options (stars, AR, ...) open a slider. All the open sliders sit in one area at the bottom whose
 * height never changes, so opening or closing one moves nothing else. The panel itself is always mounted and
 * opens by growing, which is what lets it animate both ways.
 */
export function FilterPanel(props: {
  open: boolean;
  filters: ClientFilters;
  settings: Settings | null;
  onFilters: (patch: Partial<ClientFilters>) => void;
  onSettings: (patch: Partial<Settings>) => void;
}) {
  const { filters, settings, onFilters } = props;
  // A slider that already has a setting starts open, so the panel shows what is narrowing the results.
  const [open, setOpen] = useState<ReadonlySet<SliderKey>>(
    () => new Set(SLIDERS.filter((def) => spanActive(filters.ranges[def.key])).map((def) => def.key)),
  );
  const [leaving, setLeaving] = useState<ReadonlySet<SliderKey>>(new Set());
  const timers = useRef(new Map<SliderKey, ReturnType<typeof setTimeout>>());
  useEffect(() => {
    const pending = timers.current;
    return () => pending.forEach((timer) => clearTimeout(timer));
  }, []);

  const toggle = (key: SliderKey) => {
    if (open.has(key)) {
      setOpen((current) => minus(current, key));
      setLeaving((current) => plus(current, key));
      timers.current.set(
        key,
        setTimeout(() => setLeaving((current) => minus(current, key)), LEAVE_MS),
      );
    } else {
      clearTimeout(timers.current.get(key));
      setLeaving((current) => minus(current, key));
      setOpen((current) => plus(current, key));
    }
  };

  const shown = SLIDERS.filter((def) => open.has(def.key) || leaving.has(def.key));
  const anyOn = activeFilterCount(filters) > 0;
  return (
    <div className="fwrap" data-open={props.open} inert={!props.open}>
      <div className="fclip">
        <section className="fpanel" aria-label="Filters">
          <div className="frows">
            <Row label="Difficulty">
              {TIERS.map((tier) => (
                <Chip key={tier.value} on={filters.tier === tier.value} title={tier.label} onClick={() => onFilters({ tier: tier.value })}>
                  {tier.short}
                </Chip>
              ))}
            </Row>
            <Row label="Ranges">
              {SLIDERS.map((def) => (
                <RangeChip key={def.key} def={def} span={filters.ranges[def.key]} open={open.has(def.key)} onClick={() => toggle(def.key)} />
              ))}
            </Row>
            <Row label="Genre">
              <Chip on={filters.genre === null} onClick={() => onFilters({ genre: null })}>
                Any
              </Chip>
              {GENRES.map((genre) => (
                <Chip key={genre.id} on={filters.genre === genre.id} onClick={() => onFilters({ genre: genre.id })}>
                  {genre.label}
                </Chip>
              ))}
            </Row>
            <Row label="Language">
              <Chip on={filters.language === null} onClick={() => onFilters({ language: null })}>
                Any
              </Chip>
              {LANGUAGES.map((language) => (
                <Chip key={language.id} on={filters.language === language.id} onClick={() => onFilters({ language: language.id })}>
                  {language.label}
                </Chip>
              ))}
            </Row>
            <Row label="More">
              <Chip on={filters.video} onClick={() => onFilters({ video: !filters.video })}>
                Has video
              </Chip>
              <Chip on={filters.storyboard} onClick={() => onFilters({ storyboard: !filters.storyboard })}>
                Has storyboard
              </Chip>
              <Chip on={filters.featured} onClick={() => onFilters({ featured: !filters.featured })}>
                Featured artists
              </Chip>
              <Chip on={filters.spotlight} onClick={() => onFilters({ spotlight: !filters.spotlight })}>
                Spotlighted
              </Chip>
              <Chip
                on={settings?.hideInstalled ?? false}
                title="Leave out maps that are already in your osu! library"
                onClick={() => props.onSettings({ hideInstalled: !(settings?.hideInstalled ?? false) })}
              >
                Hide installed
              </Chip>
              <Chip
                on={filters.showExplicit}
                title="Explicit maps are hidden unless this is on, as on the osu! website"
                onClick={() => onFilters({ showExplicit: !filters.showExplicit })}
              >
                Show explicit
              </Chip>
            </Row>
          </div>
          <div className="fbottom">
            <div className="sarea" aria-live="polite">
              <p className={`sempty${shown.length === 0 ? " on" : ""}`}>Choose a blue option above to set its range here</p>
              <div className="sgrid">
                {/* Every slider has a cell of its own, filled only while it is open: closing one moves no other. */}
                {SLIDERS.map((def) => (
                  <div key={def.key} className={`sslot${leaving.has(def.key) && !open.has(def.key) ? " leaving" : ""}`}>
                    {(open.has(def.key) || leaving.has(def.key)) && (
                      <RangeSlider
                        def={def}
                        span={filters.ranges[def.key]}
                        onChange={(span) => onFilters({ ranges: { ...filters.ranges, [def.key]: span } })}
                      />
                    )}
                  </div>
                ))}
              </div>
            </div>
            <div className="ffoot">
              <span className="flegend">
                <i aria-hidden /> Blue options open a slider
              </span>
              <button className="link reset" disabled={!anyOn} onClick={() => onFilters(NO_FILTERS)}>
                Reset filters
              </button>
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
