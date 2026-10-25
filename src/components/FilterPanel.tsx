import type { ReactNode } from "react";
import { GENRES, LANGUAGES, NO_FILTERS, TIERS, activeFilterCount } from "../lib/results";
import type { ClientFilters } from "../lib/results";
import type { Settings } from "../lib/types";

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
 * The osu! website's filter list, in its order and its words. What needs an osu! account (rank achieved,
 * played, favourites, subscribed mappers) can't exist here. Every filter works on the results already
 * fetched, so choosing one is instant and never starts a new search.
 */
export function FilterPanel(props: {
  filters: ClientFilters;
  settings: Settings | null;
  onFilters: (patch: Partial<ClientFilters>) => void;
  onSettings: (patch: Partial<Settings>) => void;
}) {
  const { filters, settings, onFilters } = props;
  return (
    <section className="fpanel" aria-label="Filters">
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
      <Row label="Extra">
        <Chip on={filters.video} onClick={() => onFilters({ video: !filters.video })}>
          Has video
        </Chip>
        <Chip on={filters.storyboard} onClick={() => onFilters({ storyboard: !filters.storyboard })}>
          Has storyboard
        </Chip>
      </Row>
      <Row label="General">
        <Chip on={filters.featured} onClick={() => onFilters({ featured: !filters.featured })}>
          Featured artists
        </Chip>
        <Chip on={filters.spotlight} onClick={() => onFilters({ spotlight: !filters.spotlight })}>
          Spotlighted beatmaps
        </Chip>
      </Row>
      <Row label="Library">
        <Chip
          on={settings?.hideInstalled ?? false}
          title="Leave out maps that are already in your osu! library"
          onClick={() => props.onSettings({ hideInstalled: !(settings?.hideInstalled ?? false) })}
        >
          Hide installed
        </Chip>
      </Row>
      <Row label="Explicit content">
        <Chip on={!filters.showExplicit} onClick={() => onFilters({ showExplicit: false })}>
          Hide
        </Chip>
        <Chip on={filters.showExplicit} onClick={() => onFilters({ showExplicit: true })}>
          Show
        </Chip>
      </Row>
      {activeFilterCount(filters) > 0 && (
        <button className="link reset" onClick={() => onFilters(NO_FILTERS)}>
          Reset filters
        </button>
      )}
    </section>
  );
}
