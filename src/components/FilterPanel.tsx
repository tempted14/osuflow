import type { ReactNode } from "react";
import { SLIDERS } from "../lib/ranges";
import { GENRES, LANGUAGES, NO_FILTERS, TIERS, activeFilterCount } from "../lib/results";
import type { ClientFilters } from "../lib/results";
import type { Settings } from "../lib/types";
import { RangeSlider } from "./RangeSlider";

function Chip(props: { on: boolean; onClick: () => void; title?: string; children: ReactNode }) {
  return (
    <button className={`chip${props.on ? " on" : ""}`} aria-pressed={props.on} title={props.title} onClick={props.onClick}>
      {props.children}
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

/**
 * The osu! website's filter list, in its order and its words, made compact. What needs an osu! account (rank
 * achieved, played, favourites, subscribed mappers) can't exist here. Every filter works on the results already
 * fetched, so choosing one is instant and never starts a new search.
 *
 * The sliders (stars, AR, CS, OD, HP, BPM, length) are always shown in one block at the bottom, in a grid that
 * never changes, so using one moves nothing. The panel itself is always mounted and opens by growing, which is
 * what lets it animate both ways.
 */
export function FilterPanel(props: {
  open: boolean;
  filters: ClientFilters;
  settings: Settings | null;
  onFilters: (patch: Partial<ClientFilters>) => void;
  onSettings: (patch: Partial<Settings>) => void;
}) {
  const { filters, settings, onFilters } = props;
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
            <div className="frow">
              <span className="flabel">Ranges</span>
              <div className="sgrid">
                {SLIDERS.map((def) => (
                  <RangeSlider
                    key={def.key}
                    def={def}
                    span={filters.ranges[def.key]}
                    onChange={(span) => onFilters({ ranges: { ...filters.ranges, [def.key]: span } })}
                  />
                ))}
              </div>
            </div>
            <div className="ffoot">
              <button className="link reset" disabled={activeFilterCount(filters) === 0} onClick={() => onFilters(NO_FILTERS)}>
                Reset filters
              </button>
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
