import { describe, expect, it } from "vitest";
import { MENU_NAV_KEYS, stepIndex, typeAhead } from "./menu";
import { actingIndex } from "./nav";
import { SLIDERS, spanShort } from "./ranges";
import type { SliderDef } from "./ranges";

const slider = (key: SliderDef["key"]) => SLIDERS.find((def) => def.key === key) as SliderDef;

describe("moving through a menu", () => {
  it("steps by one and by a page, and stops at the ends", () => {
    expect(stepIndex(2, "ArrowDown", 6)).toBe(3);
    expect(stepIndex(2, "ArrowUp", 6)).toBe(1);
    expect(stepIndex(5, "ArrowDown", 6)).toBe(5);
    expect(stepIndex(0, "ArrowUp", 6)).toBe(0);
    expect(stepIndex(1, "PageDown", 11)).toBe(6);
    expect(stepIndex(8, "PageDown", 11)).toBe(10);
    expect(stepIndex(8, "PageUp", 11)).toBe(3);
    expect(stepIndex(2, "PageUp", 11)).toBe(0);
  });

  it("jumps to either end", () => {
    expect(stepIndex(3, "Home", 6)).toBe(0);
    expect(stepIndex(3, "End", 6)).toBe(5);
  });

  it("leaves the highlight alone for other keys, and for an empty menu", () => {
    expect(stepIndex(3, "a", 6)).toBe(3);
    expect(stepIndex(0, "ArrowDown", 0)).toBe(-1);
    expect(MENU_NAV_KEYS).toContain("Home");
  });

  it("jumps to the entry a typed word starts", () => {
    const labels = ["Best match", "Ranked date", "Last updated", "Submitted", "Plays", "Favourites", "Stars", "BPM", "Length", "Title", "Artist"];
    expect(typeAhead(labels, "ti", 0)).toBe(9);
    expect(typeAhead(labels, "TITLE", 0)).toBe(9);
    expect(typeAhead(labels, "z", 0)).toBe(-1);
    expect(typeAhead(labels, "", 0)).toBe(-1);
  });

  it("steps through the entries that share a first letter when one letter is repeated", () => {
    const labels = ["Stars", "BPM", "Submitted", "Plays", "Sort"];
    expect(typeAhead(labels, "s", 0)).toBe(2);
    expect(typeAhead(labels, "s", 2)).toBe(4);
    expect(typeAhead(labels, "ss", 4)).toBe(0);
    expect(typeAhead(labels, "s", 3)).toBe(4);
  });
});

describe("what Enter acts on", () => {
  const base = { selected: -1, hovered: -1, pointerAt: 0, keyAt: 0 };

  it("is the pointed-at card once the pointer has moved since the last key", () => {
    expect(actingIndex({ ...base, hovered: 4, pointerAt: 200, keyAt: 100 })).toBe(4);
    expect(actingIndex({ ...base, selected: 2, hovered: 4, pointerAt: 200, keyAt: 100 })).toBe(4);
  });

  it("is the card chosen with the arrows while the keys were used last", () => {
    expect(actingIndex({ ...base, selected: 2, hovered: 4, pointerAt: 100, keyAt: 200 })).toBe(2);
  });

  it("never takes a card the pointer merely rests on after typing", () => {
    // Typing a search, then Enter, with the pointer parked over a result: nothing is picked.
    expect(actingIndex({ ...base, hovered: 4, pointerAt: 100, keyAt: 200 })).toBe(-1);
    expect(actingIndex({ ...base, hovered: 4, pointerAt: 100, keyAt: 100 })).toBe(-1);
  });

  it("is nothing when nothing is pointed at or selected", () => {
    expect(actingIndex({ ...base, pointerAt: 200, keyAt: 100 })).toBe(-1);
  });
});

describe("a slider on a chip", () => {
  it("is as short as it can be", () => {
    expect(spanShort(slider("stars"), { min: null, max: null })).toBe("");
    expect(spanShort(slider("stars"), { min: 4, max: 6.2 })).toBe("4.0★–6.2★");
    expect(spanShort(slider("ar"), { min: 9, max: null })).toBe("9.0+");
    expect(spanShort(slider("length"), { min: null, max: 240 })).toBe("≤ 4:00");
    expect(spanShort(slider("bpm"), { min: 180, max: 180 })).toBe("180");
  });
});
