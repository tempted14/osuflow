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
import { autoHideMs, browserName, linkMode, toastLine, toastProgress } from "./links";
import {
  applyProgressEvent,
  applyStateEvent,
  isBusy,
  pillText,
  removeFinished,
  removeJob,
  summarizeDownloads,
  upsertJob,
} from "./jobs";
import { isNavKey, moveSelection } from "./nav";
import { FRESH_FOR_MS, applyLibraryDelta, mergeResults, planFetch, searchKey, visibleResults } from "./results";
import type { FetchSituation } from "./results";
import { starColor } from "./starColor";
import type {
  Beatmap,
  Beatmapset,
  DownloadJob,
  DownloadProgressEvent,
  DownloadStateEvent,
  LinkHandlerStatus,
  LinkToast,
} from "./types";

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

describe("the downloads pill", () => {
  const job = (patch: Partial<DownloadJob> = {}): DownloadJob => ({
    setId: 1, label: "A – B", state: "downloading", received: 0, total: null, error: null, mirror: null, ...patch,
  });
  const text = (jobs: DownloadJob[]) => pillText(summarizeDownloads(jobs));

  it("says nothing when there is nothing to say", () => {
    expect(text([])).toBe("");
  });

  it("describes what is running, with the overall percentage once a size is known", () => {
    expect(text([job({ state: "queued" })])).toBe("Downloading");
    expect(text([job({ received: 25, total: 100 })])).toBe("Downloading · 25%");
    expect(text([job({ received: 50, total: 100 }), job({ setId: 2, received: 100, total: 100 })])).toBe("2 downloading · 75%");
  });

  it("counts a download that hasn't reported a size as nothing done, so the percentage never jumps back", () => {
    const summary = summarizeDownloads([job({ received: 100, total: 100 }), job({ setId: 2, state: "queued" })]);
    expect(summary.active).toBe(2);
    expect(summary.fraction).toBe(0.5);
  });

  it("leads with running downloads, then failures, then what was sent", () => {
    const running = job({ received: 10, total: 100 });
    const failed = job({ setId: 2, state: "failed", error: "boom" });
    const sent = job({ setId: 3, state: "done" });
    expect(text([running, failed, sent])).toBe("Downloading · 10%");
    expect(text([failed, sent])).toBe("1 failed");
    expect(text([failed, job({ setId: 4, state: "failed" }), sent])).toBe("2 failed");
    expect(text([sent, job({ setId: 5, state: "done" })])).toBe("2 sent to osu! ✓");
  });

  it("ignores cancelled downloads", () => {
    expect(text([job({ state: "cancelled" })])).toBe("");
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

describe("when the overlay's search fetches", () => {
  const key = searchKey({ text: "freedom dive", mode: null, status: 1, sort: "relevance" });
  const situation = (patch: Partial<FetchSituation> = {}): FetchSituation => ({
    enabled: true, typing: false, last: null, inFlightKey: null, key, now: 100_000, hadError: false, ...patch,
  });

  it("treats the same query the way the backend does", () => {
    const base = { text: "Freedom Dive", mode: null, status: 1, sort: "relevance" };
    expect(searchKey({ ...base, text: "  freedom dive " })).toBe(searchKey(base));
    expect(searchKey({ ...base, mode: 0 })).not.toBe(searchKey(base));
    expect(searchKey({ ...base, status: 4 })).not.toBe(searchKey(base));
    expect(searchKey({ ...base, sort: "plays" })).not.toBe(searchKey(base));
  });

  it("fetches the first results while the overlay is still hidden, once", () => {
    expect(planFetch(situation({ enabled: false }))).toBe("now");
    expect(planFetch(situation({ enabled: false, inFlightKey: key }))).toBe("skip");
    expect(planFetch(situation({ enabled: false, last: { key, at: 99_000 } }))).toBe("skip");
  });

  it("waits for a fetch that is already on its way instead of restarting it", () => {
    expect(planFetch(situation({ inFlightKey: key }))).toBe("skip");
    // A different query in flight is not this one.
    expect(planFetch(situation({ inFlightKey: "other" }))).toBe("now");
  });

  it("reopens on fresh results without asking again, and refetches stale ones", () => {
    const fresh = { key, at: 100_000 - (FRESH_FOR_MS - 1) };
    const stale = { key, at: 100_000 - FRESH_FOR_MS };
    expect(planFetch(situation({ last: fresh }))).toBe("skip");
    expect(planFetch(situation({ last: stale }))).toBe("now");
  });

  it("refetches when the last attempt failed, or was for another query, or never happened", () => {
    const fresh = { key, at: 99_000 };
    expect(planFetch(situation({ last: fresh, hadError: true }))).toBe("now");
    expect(planFetch(situation({ last: { ...fresh, key: "other" } }))).toBe("now");
    expect(planFetch(situation({ last: null }))).toBe("now");
  });

  it("debounces typing, whatever was fetched before", () => {
    expect(planFetch(situation({ typing: true, last: { key, at: 99_999 } }))).toBe("debounced");
    expect(planFetch(situation({ typing: true, inFlightKey: key }))).toBe("debounced");
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

describe("link toast", () => {
  const toast = (patch: Partial<LinkToast> = {}): LinkToast => ({
    state: "working", setId: 5, label: "A - B", message: "Downloading…", url: "https://osu.ppy.sh/s/5", ...patch,
  });
  const stage = (state: DownloadStateEvent["state"], setId = 5): DownloadStateEvent => ({ setId, state, error: null });
  const progress = (received: number, total: number | null, setId = 5): DownloadProgressEvent => ({
    setId, received, total, mirror: "osu.direct",
  });

  it("hides finished toasts by itself and keeps the rest until dismissed", () => {
    expect(autoHideMs("installed")).toBe(4000);
    expect(autoHideMs("done")).toBe(4000);
    expect(autoHideMs("failed")).toBe(15000);
    expect(autoHideMs("resolving")).toBeNull();
    expect(autoHideMs("working")).toBeNull();
    expect(autoHideMs("hidden")).toBeNull();
  });

  it("reports progress only for this toast's own running download", () => {
    expect(toastProgress(toast(), progress(25, 100))).toBe(0.25);
    expect(toastProgress(toast(), progress(150, 100))).toBe(1);
    expect(toastProgress(toast(), progress(25, null))).toBeNull();
    expect(toastProgress(toast(), progress(25, 0))).toBeNull();
    expect(toastProgress(toast(), progress(25, 100, 9))).toBeNull();
    expect(toastProgress(toast(), null)).toBeNull();
    expect(toastProgress(toast({ state: "done" }), progress(25, 100))).toBeNull();
  });

  it("describes what the running download is doing", () => {
    expect(toastLine(toast(), null, null)).toBe("Downloading…");
    expect(toastLine(toast(), stage("queued"), null)).toBe("Waiting for another download…");
    expect(toastLine(toast(), stage("downloading"), null)).toBe("Downloading…");
    expect(toastLine(toast(), stage("downloading"), progress(1, 3))).toBe("Downloading… 33%");
    expect(toastLine(toast(), stage("importing"), progress(3, 3))).toBe("Sending to osu!…");
  });

  it("ignores events about other maps and keeps non-working messages as they are", () => {
    expect(toastLine(toast(), stage("importing", 9), null)).toBe("Downloading…");
    expect(toastLine(toast({ state: "failed", message: "Every mirror failed" }), stage("importing"), null)).toBe(
      "Every mirror failed",
    );
    expect(toastLine(toast({ state: "installed", message: null }), null, null)).toBe("");
  });

  it("names the link handler's mode from the two switches", () => {
    const status = (enabled: boolean, isDefault: boolean): LinkHandlerStatus => ({
      enabled, registered: enabled || isDefault, isDefault, previousBrowser: null,
    });
    expect(linkMode(status(false, false))).toBe("off");
    expect(linkMode(status(true, false))).toBe("waiting");
    expect(linkMode(status(true, true))).toBe("active");
    expect(linkMode(status(false, true))).toBe("forwarding");
  });

  it("turns a browser path into a friendly name", () => {
    expect(browserName("C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe")).toBe("Chrome");
    expect(browserName("C:/Program Files (x86)/Microsoft/Edge/Application/MSEDGE.EXE")).toBe("Edge");
    expect(browserName("D:\\Tools\\Zen\\zen.exe")).toBe("zen");
    expect(browserName(".exe")).toBe("your usual browser");
    expect(browserName(null)).toBe("your usual browser");
  });
});
