import { describe, expect, it } from "vitest";
import {
  compactNumber,
  downloadFraction,
  downloadLabel,
  formatLength,
  longestLength,
  mirrorLabel,
  setLabel,
  statusLabel,
} from "./format";
import { hotkeyFromEvent } from "./hotkey";
import { applyProgressEvent, applyStateEvent, isBusy, removeFinished, removeJob, upsertJob } from "./jobs";
import { isNavKey, moveSelection } from "./nav";
import { applyLibraryDelta, mergeResults, visibleResults } from "./results";
import { starColor } from "./starColor";
import type { Beatmap, Beatmapset, DownloadJob } from "./types";

const set = (id: number, patch: Partial<Beatmapset> = {}) => ({ id, ...patch }) as Beatmapset;

describe("format", () => {
  it("formats lengths as m:ss", () => {
    expect(formatLength(0)).toBe("0:00");
    expect(formatLength(59)).toBe("0:59");
    expect(formatLength(187)).toBe("3:07");
    expect(formatLength(61.6)).toBe("1:02");
    expect(formatLength(3600)).toBe("60:00");
    expect(formatLength(-5)).toBe("0:00");
  });

  it("abbreviates big numbers", () => {
    expect(compactNumber(0)).toBe("0");
    expect(compactNumber(999)).toBe("999");
    expect(compactNumber(1234)).toBe("1.2K");
    expect(compactNumber(15000)).toBe("15K");
    expect(compactNumber(2_500_000)).toBe("2.5M");
  });

  it("labels statuses, sets and mirrors", () => {
    expect(statusLabel("ranked")).toBe("Ranked");
    expect(statusLabel("wip")).toBe("WIP");
    expect(statusLabel("")).toBe("");
    expect(setLabel({ artist: "xi", title: "FREEDOM DiVE" })).toBe("xi – FREEDOM DiVE");
    expect(mirrorLabel("osuDirect")).toBe("osu.direct");
    expect(mirrorLabel("catboy")).toBe("catboy.best");
  });

  it("uses the longest difficulty as the set length", () => {
    const maps = [{ total_length: 90 }, { total_length: 140 }, { total_length: 120 }] as Beatmap[];
    expect(longestLength({ beatmaps: maps })).toBe(140);
    expect(longestLength({ beatmaps: [] })).toBe(0);
  });

  it("describes download progress", () => {
    expect(downloadLabel({ state: "queued", received: 0, total: null })).toBe("Queued");
    expect(downloadLabel({ state: "downloading", received: 512, total: 1024 })).toBe("50%");
    expect(downloadLabel({ state: "downloading", received: 3 * 1024 * 1024, total: null })).toBe("3.0 MB");
    expect(downloadLabel({ state: "importing", received: 1, total: 1 })).toBe("Sending to osu!…");
    expect(downloadLabel({ state: "done", received: 1, total: 1 })).toBe("Sent to osu!");
    expect(downloadLabel({ state: "failed", received: 0, total: null })).toBe("Failed");
    expect(downloadLabel({ state: "cancelled", received: 0, total: null })).toBe("Cancelled");
  });

  it("turns progress into a bar fraction", () => {
    expect(downloadFraction({ state: "downloading", received: 256, total: 1024 })).toBe(0.25);
    expect(downloadFraction({ state: "downloading", received: 2048, total: 1024 })).toBe(1);
    expect(downloadFraction({ state: "downloading", received: 5, total: null })).toBeNull();
    expect(downloadFraction({ state: "importing", received: 1, total: 1 })).toBe(1);
    expect(downloadFraction({ state: "queued", received: 0, total: null })).toBeNull();
  });
});

describe("starColor", () => {
  it("returns the spectrum's own colours at its stops", () => {
    expect(starColor(0.1)).toBe("#4290fb");
    expect(starColor(2)).toBe("#4fffd5");
    expect(starColor(6.7)).toBe("#6563de");
  });

  it("blends between stops", () => {
    expect(starColor(1.625)).toBe("#4fe0ea");
  });

  it("clamps at both ends and survives bad input", () => {
    expect(starColor(0.05)).toBe("#aaaaaa");
    expect(starColor(0)).toBe("#aaaaaa");
    expect(starColor(9)).toBe("#000000");
    expect(starColor(15)).toBe("#000000");
    expect(starColor(Number.NaN)).toBe("#aaaaaa");
  });
});

describe("moveSelection", () => {
  // Seven cards in three columns: rows [0 1 2] [3 4 5] [6].
  const move = (from: number, key: Parameters<typeof moveSelection>[1]) => moveSelection(from, key, 3, 7);

  it("enters the grid from the search box and leaves it upwards", () => {
    expect(move(-1, "ArrowDown")).toBe(0);
    expect(move(-1, "ArrowUp")).toBe(-1);
    expect(move(1, "ArrowUp")).toBe(-1);
  });

  it("moves along rows and between them", () => {
    expect(move(0, "ArrowRight")).toBe(1);
    expect(move(0, "ArrowLeft")).toBe(0);
    expect(move(5, "ArrowRight")).toBe(6);
    expect(move(4, "ArrowLeft")).toBe(3);
    expect(move(0, "ArrowDown")).toBe(3);
    expect(move(4, "ArrowUp")).toBe(1);
  });

  it("lands on the last card when the last row is short", () => {
    expect(move(4, "ArrowDown")).toBe(6);
    expect(move(5, "ArrowDown")).toBe(6);
    expect(move(6, "ArrowDown")).toBe(6);
  });

  it("copes with empty grids, stale selections and silly column counts", () => {
    expect(moveSelection(0, "ArrowDown", 3, 0)).toBe(-1);
    expect(moveSelection(9, "ArrowDown", 3, 5)).toBe(0);
    expect(moveSelection(1, "ArrowDown", 0, 4)).toBe(2);
  });

  it("recognises arrow keys", () => {
    expect(isNavKey("ArrowLeft")).toBe(true);
    expect(isNavKey("Enter")).toBe(false);
  });
});

describe("download jobs", () => {
  const job = (patch: Partial<DownloadJob> = {}): DownloadJob => ({
    setId: 1, label: "A – B", state: "downloading", received: 0, total: null, error: null, mirror: null, ...patch,
  });

  it("creates a job when an event arrives for an unknown set", () => {
    const jobs = applyStateEvent([], { setId: 7, state: "queued", error: null });
    expect(jobs).toEqual([expect.objectContaining({ setId: 7, label: "Beatmapset 7", state: "queued" })]);
  });

  it("updates in place and keeps order", () => {
    const jobs = upsertJob([job({ setId: 1 }), job({ setId: 2 })], 1, { received: 10 });
    expect(jobs.map((j) => j.setId)).toEqual([1, 2]);
    expect(jobs[0].received).toBe(10);
    expect(jobs[1].received).toBe(0);
  });

  it("starts a new attempt cleanly when queued again", () => {
    const old = job({ state: "failed", received: 500, total: 1000, error: "boom", mirror: "osu.direct" });
    const [next] = applyStateEvent([old], { setId: 1, state: "queued", error: null });
    expect(next).toMatchObject({ state: "queued", received: 0, total: null, error: null, mirror: null });
  });

  it("records progress and failures", () => {
    const progressed = applyProgressEvent([job()], { setId: 1, received: 5, total: 10, mirror: "catboy.best" });
    expect(progressed[0]).toMatchObject({ received: 5, total: 10, mirror: "catboy.best" });
    const failed = applyStateEvent(progressed, { setId: 1, state: "failed", error: "every mirror failed" });
    expect(failed[0]).toMatchObject({ state: "failed", error: "every mirror failed", received: 5 });
  });

  it("tells busy jobs from finished ones and removes them", () => {
    const jobs = [job({ setId: 1, state: "queued" }), job({ setId: 2, state: "done" }), job({ setId: 3, state: "failed" })];
    expect(jobs.map(isBusy)).toEqual([true, false, false]);
    expect(removeFinished(jobs).map((j) => j.setId)).toEqual([1]);
    expect(removeJob(jobs, 2).map((j) => j.setId)).toEqual([1, 3]);
  });
});

describe("results", () => {
  it("merges pages without duplicates", () => {
    expect(mergeResults([set(1), set(2)], [set(2), set(3)]).map((s) => s.id)).toEqual([1, 2, 3]);
    expect(mergeResults([], [set(5)]).map((s) => s.id)).toEqual([5]);
  });

  it("hides installed sets only when asked, keeping ones with downloads", () => {
    const all = [set(1), set(2), set(3)];
    const installed = new Set([1, 2]);
    expect(visibleResults(all, installed, false, new Set()).map((s) => s.id)).toEqual([1, 2, 3]);
    expect(visibleResults(all, installed, true, new Set()).map((s) => s.id)).toEqual([3]);
    expect(visibleResults(all, installed, true, new Set([2])).map((s) => s.id)).toEqual([2, 3]);
  });

  it("applies library deltas without mutating the old set", () => {
    const before = new Set([1, 2, 3]);
    const after = applyLibraryDelta(before, { added: [9], removed: [2] });
    expect([...after].sort()).toEqual([1, 3, 9]);
    expect([...before].sort()).toEqual([1, 2, 3]);
  });
});

describe("hotkeyFromEvent", () => {
  const chord = (code: string, mods: Partial<Record<"ctrlKey" | "altKey" | "shiftKey" | "metaKey", boolean>> = {}) => ({
    code, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...mods,
  });

  it("spells letters, digits and function keys the way the backend parses them", () => {
    expect(hotkeyFromEvent(chord("KeyD", { ctrlKey: true, shiftKey: true }))).toBe("Ctrl+Shift+D");
    expect(hotkeyFromEvent(chord("Digit1", { ctrlKey: true }))).toBe("Ctrl+1");
    expect(hotkeyFromEvent(chord("F8"))).toBe("F8");
    expect(hotkeyFromEvent(chord("Space", { altKey: true }))).toBe("Alt+Space");
    expect(hotkeyFromEvent(chord("Comma", { ctrlKey: true, altKey: true }))).toBe("Ctrl+Alt+Comma");
    expect(hotkeyFromEvent(chord("KeyK", { metaKey: true }))).toBe("Super+K");
  });

  it("waits for a real key while only modifiers are held", () => {
    expect(hotkeyFromEvent(chord("ControlLeft", { ctrlKey: true }))).toBeNull();
    expect(hotkeyFromEvent(chord("ShiftRight", { shiftKey: true }))).toBeNull();
    expect(hotkeyFromEvent(chord("AltLeft", { altKey: true }))).toBeNull();
    expect(hotkeyFromEvent(chord("MetaLeft", { metaKey: true }))).toBeNull();
  });
});
