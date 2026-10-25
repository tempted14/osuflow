import { formatLength } from "./format";
import type { NumericConstraint, NumericKey } from "./query";

/**
 * The sliders in the Filters panel: the same conditions as `stars>5` and friends in the search box, set by
 * dragging. Each is a pair of handles; a handle at the end of its track means "no limit" on that side, so a
 * slider left at full width filters nothing.
 */
export type SliderKey = "stars" | "ar" | "cs" | "od" | "hp" | "bpm" | "length";

/** What is asked of one difficulty property: null on a side means no limit there. */
export interface Span {
  min: number | null;
  max: number | null;
}

export type Ranges = Record<SliderKey, Span>;

export interface SliderDef {
  key: SliderKey;
  label: string;
  /** The track's ends. The handle resting on one is "no limit", so they bound what is worth asking for. */
  min: number;
  max: number;
  step: number;
  format: (value: number) => string;
  /** Spoken and hover text, for the control as a whole. */
  hint: string;
}

const oneDecimal = (value: number) => value.toFixed(1);

export const SLIDERS: SliderDef[] = [
  { key: "stars", label: "Stars", min: 0, max: 10, step: 0.1, format: (v) => `${oneDecimal(v)}★`, hint: "Star rating of a difficulty" },
  { key: "ar", label: "AR", min: 0, max: 10, step: 0.1, format: oneDecimal, hint: "Approach rate" },
  { key: "cs", label: "CS", min: 0, max: 10, step: 0.1, format: oneDecimal, hint: "Circle size (the key count in mania)" },
  { key: "od", label: "OD", min: 0, max: 10, step: 0.1, format: oneDecimal, hint: "Overall difficulty" },
  { key: "hp", label: "HP", min: 0, max: 10, step: 0.1, format: oneDecimal, hint: "HP drain" },
  { key: "bpm", label: "BPM", min: 60, max: 300, step: 5, format: (v) => String(Math.round(v)), hint: "Beats per minute" },
  { key: "length", label: "Length", min: 0, max: 600, step: 10, format: formatLength, hint: "Length of the map" },
];

export const NO_SPAN: Span = { min: null, max: null };

export const NO_RANGES: Ranges = {
  stars: NO_SPAN,
  ar: NO_SPAN,
  cs: NO_SPAN,
  od: NO_SPAN,
  hp: NO_SPAN,
  bpm: NO_SPAN,
  length: NO_SPAN,
};

export const spanActive = (span: Span): boolean => span.min !== null || span.max !== null;

/** How many sliders are set: they add to the number on the Filters button. */
export function activeRangeCount(ranges: Ranges): number {
  return SLIDERS.filter((slider) => spanActive(ranges[slider.key])).length;
}

/** The conditions the sliders stand for, in the form the search box's conditions are matched with. */
export function rangeConstraints(ranges: Ranges): NumericConstraint[] {
  return SLIDERS.flatMap(({ key }): NumericConstraint[] => {
    const { min, max } = ranges[key];
    if (min === null && max === null) return [];
    const numeric: NumericKey = key;
    return [{ key: numeric, min: min ?? -Infinity, max: max ?? Infinity, minOpen: false, maxOpen: false }];
  });
}

/** What a handle position means: a handle on the end of the track is no limit at all. */
export function spanFromHandles(def: SliderDef, low: number, high: number): Span {
  const [from, to] = low <= high ? [low, high] : [high, low];
  return { min: from <= def.min ? null : from, max: to >= def.max ? null : to };
}

/** Where the handles sit for a span. */
export function handlesOf(def: SliderDef, span: Span): [number, number] {
  return [span.min ?? def.min, span.max ?? def.max];
}

/** The current setting in words: "Any", "4.5★ – 6.2★", "8.0 and up", "up to 3:00". */
export function spanLabel(def: SliderDef, span: Span): string {
  const { min, max } = span;
  if (min === null && max === null) return "Any";
  if (min === null) return `up to ${def.format(max as number)}`;
  if (max === null) return `${def.format(min)} and up`;
  return min === max ? def.format(min) : `${def.format(min)} – ${def.format(max)}`;
}

/**
 * Makes a saved or typed span safe to use: numbers only, inside the track, the right way round, and
 * with the track's own ends turned back into "no limit". Anything unusable means no limit.
 */
export function sanitizeSpan(def: SliderDef, raw: unknown): Span {
  if (!raw || typeof raw !== "object") return NO_SPAN;
  const { min, max } = raw as Record<string, unknown>;
  const usable = (value: unknown) => (typeof value === "number" && Number.isFinite(value) ? value : null);
  const [low, high] = [usable(min), usable(max)];
  if (low !== null && high !== null) {
    return spanFromHandles(def, Math.min(Math.max(low, def.min), def.max), Math.min(Math.max(high, def.min), def.max));
  }
  return {
    min: low === null || low <= def.min ? null : Math.min(low, def.max),
    max: high === null || high >= def.max ? null : Math.max(high, def.min),
  };
}
