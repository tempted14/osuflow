import { describe, expect, it } from "vitest";
import { parsePrefs } from "./prefs";
import { parseQuery } from "./query";
import {
  NO_RANGES,
  NO_SPAN,
  SLIDERS,
  activeRangeCount,
  handlesOf,
  rangeConstraints,
  sanitizeSpan,
  spanFromHandles,
  spanLabel,
} from "./ranges";
import type { Ranges, SliderDef } from "./ranges";
import { NO_FILTERS, activeFilterCount, matchesFilters, narrowsResults } from "./results";
import type { Beatmap, Beatmapset } from "./types";

const slider = (key: SliderDef["key"]) => SLIDERS.find((def) => def.key === key) as SliderDef;
const withRanges = (patch: Partial<Ranges>) => ({ ...NO_FILTERS, ranges: { ...NO_RANGES, ...patch } });

const difficulty = (patch: Partial<Beatmap> = {}) =>
  ({ difficulty_rating: 5, ar: 9, cs: 4, accuracy: 8, drain: 5, bpm: 180, total_length: 120, mode_int: 0, ...patch }) as Beatmap;
const setOf = (beatmaps: Beatmap[]) => ({ id: 1, nsfw: false, beatmaps }) as Beatmapset;

describe("what the handles mean", () => {
  it("treats a handle on the end of the track as no limit", () => {
    const stars = slider("stars");
    expect(spanFromHandles(stars, 0, 10)).toEqual(NO_SPAN);
    expect(spanFromHandles(stars, 4.5, 10)).toEqual({ min: 4.5, max: null });
    expect(spanFromHandles(stars, 0, 6.2)).toEqual({ min: null, max: 6.2 });
    expect(spanFromHandles(stars, 4.5, 6.2)).toEqual({ min: 4.5, max: 6.2 });
  });

  it("puts the handles the right way round and lets them meet", () => {
    expect(spanFromHandles(slider("ar"), 9.5, 8)).toEqual({ min: 8, max: 9.5 });
    expect(spanFromHandles(slider("ar"), 9, 9)).toEqual({ min: 9, max: 9 });
  });

  it("puts the handles back on the track's ends for an open side", () => {
    const bpm = slider("bpm");
    expect(handlesOf(bpm, NO_SPAN)).toEqual([60, 300]);
    expect(handlesOf(bpm, { min: 180, max: null })).toEqual([180, 300]);
    expect(handlesOf(bpm, { min: null, max: 200 })).toEqual([60, 200]);
  });
});

describe("saying what is set", () => {
  it("reads out each kind of setting", () => {
    const stars = slider("stars");
    expect(spanLabel(stars, NO_SPAN)).toBe("Any");
    expect(spanLabel(stars, { min: 4.5, max: 6.2 })).toBe("4.5★ – 6.2★");
    expect(spanLabel(stars, { min: 7, max: null })).toBe("7.0★ and up");
    expect(spanLabel(stars, { min: null, max: 3 })).toBe("up to 3.0★");
    expect(spanLabel(stars, { min: 5, max: 5 })).toBe("5.0★");
  });

  it("writes lengths as minutes and BPM as whole numbers", () => {
    expect(spanLabel(slider("length"), { min: 90, max: 180 })).toBe("1:30 – 3:00");
    expect(spanLabel(slider("length"), { min: null, max: 180 })).toBe("up to 3:00");
    expect(spanLabel(slider("bpm"), { min: 180, max: 200 })).toBe("180 – 200");
  });
});

describe("saved sliders", () => {
  it("keeps good values and drops the rest", () => {
    const ar = slider("ar");
    expect(sanitizeSpan(ar, { min: 8, max: 9.5 })).toEqual({ min: 8, max: 9.5 });
    expect(sanitizeSpan(ar, { min: 9.5, max: 8 })).toEqual({ min: 8, max: 9.5 });
    expect(sanitizeSpan(ar, { min: "8", max: NaN })).toEqual(NO_SPAN);
    expect(sanitizeSpan(ar, null)).toEqual(NO_SPAN);
    expect(sanitizeSpan(ar, "nonsense")).toEqual(NO_SPAN);
    expect(sanitizeSpan(ar, { min: 8 })).toEqual({ min: 8, max: null });
  });

  it("pulls values back inside the track and turns its ends into no limit", () => {
    const ar = slider("ar");
    expect(sanitizeSpan(ar, { min: -5, max: 99 })).toEqual(NO_SPAN);
    expect(sanitizeSpan(ar, { min: 0, max: 9 })).toEqual({ min: null, max: 9 });
    expect(sanitizeSpan(ar, { min: 99, max: null })).toEqual({ min: 10, max: null });
  });

  it("is read back with the rest of the saved choices", () => {
    const saved = JSON.stringify({ filters: { ranges: { stars: { min: 5, max: 6 }, ar: { min: 9, max: null }, bpm: "junk" } } });
    const { ranges } = parsePrefs(saved).filters;
    expect(ranges.stars).toEqual({ min: 5, max: 6 });
    expect(ranges.ar).toEqual({ min: 9, max: null });
    expect(ranges.bpm).toEqual(NO_SPAN);
    expect(ranges.cs).toEqual(NO_SPAN);
  });

  it("loads choices saved before the sliders existed, with every slider open", () => {
    const { ranges } = parsePrefs(JSON.stringify({ filters: { genre: 5 } })).filters;
    expect(ranges).toEqual(NO_RANGES);
    expect(parsePrefs(null).filters.ranges).toEqual(NO_RANGES);
    expect(parsePrefs(JSON.stringify({ filters: { ranges: 7 } })).filters.ranges).toEqual(NO_RANGES);
  });
});

describe("filtering with sliders", () => {
  it("turns each set slider into a condition, and leaves open ones out", () => {
    expect(rangeConstraints(NO_RANGES)).toEqual([]);
    const constraints = rangeConstraints({ ...NO_RANGES, stars: { min: 4, max: 6 }, ar: { min: 9, max: null } });
    expect(constraints).toHaveLength(2);
    expect(constraints[0]).toMatchObject({ key: "stars", min: 4, max: 6 });
    expect(constraints[1]).toMatchObject({ key: "ar", min: 9, max: Infinity });
  });

  it("counts each set slider once on the Filters button, and as narrowing", () => {
    expect(activeRangeCount(NO_RANGES)).toBe(0);
    const filters = withRanges({ stars: { min: 4, max: 6 }, ar: { min: 9, max: null } });
    expect(activeRangeCount(filters.ranges)).toBe(2);
    expect(activeFilterCount(filters)).toBe(2);
    expect(activeFilterCount({ ...filters, video: true })).toBe(3);
    expect(activeFilterCount(NO_FILTERS)).toBe(0);
    expect(narrowsResults(filters)).toBe(true);
    expect(narrowsResults(NO_FILTERS)).toBe(false);
  });

  it("keeps the sets that have a difficulty inside every slider", () => {
    const set = setOf([difficulty({ difficulty_rating: 6.2, ar: 9.7, bpm: 150, total_length: 317 })]);
    expect(matchesFilters(set, withRanges({ ar: { min: 9.5, max: null } }), null)).toBe(true);
    expect(matchesFilters(set, withRanges({ ar: { min: null, max: 9 } }), null)).toBe(false);
    expect(matchesFilters(set, withRanges({ bpm: { min: 140, max: 160 } }), null)).toBe(true);
    expect(matchesFilters(set, withRanges({ length: { min: null, max: 300 } }), null)).toBe(false);
    expect(matchesFilters(set, withRanges({ stars: { min: 6, max: 6.5 } }), null)).toBe(true);
  });

  it("needs one difficulty to satisfy all the sliders, and the typed conditions too", () => {
    const set = setOf([
      difficulty({ difficulty_rating: 5.2, ar: 9.2 }),
      difficulty({ difficulty_rating: 3.2, ar: 7.5 }),
    ]);
    const hardAndSlow = withRanges({ stars: { min: 5, max: null }, ar: { min: null, max: 8 } });
    expect(matchesFilters(set, hardAndSlow, null)).toBe(false);
    expect(matchesFilters(set, withRanges({ stars: { min: 5, max: null } }), null, parseQuery("ar>9"))).toBe(true);
    expect(matchesFilters(set, withRanges({ stars: { min: 5, max: null } }), null, parseQuery("ar<8"))).toBe(false);
  });

  it("filters nothing when every slider is open", () => {
    expect(matchesFilters(setOf([difficulty()]), NO_FILTERS, null)).toBe(true);
    expect(matchesFilters(setOf([]), NO_FILTERS, null)).toBe(true);
  });
});
