import { describe, expect, it } from "vitest";
import { downloadLabel } from "./format";
import { newJob, applyStateEvent, pillText, summarizeDownloads } from "./jobs";

describe("client-specific handoff feedback", () => {
  it("keeps the stable refresh instruction out of lazer recovery", () => {
    const lazer = { ...newJob(1), state: "done" as const, importMethod: "savedForLazer" as const };
    const stable = { ...newJob(2), state: "done" as const, importMethod: "movedToSongs" as const };
    expect(downloadLabel(lazer)).toContain("open in osu!lazer"); expect(downloadLabel(lazer)).not.toContain("F5");
    expect(downloadLabel(stable)).toContain("F5");
    expect(summarizeDownloads([lazer]).lazerManual).toBe(1);
    expect(pillText(summarizeDownloads([lazer]))).toContain("open in osu!lazer");
  });
  it("treats a native launch as sent, and clears an old recovery method when retrying", () => {
    const job = { ...newJob(1), state: "done" as const, importMethod: "openedInLazer" as const };
    expect(downloadLabel(job)).toBe("Sent to osu!lazer");
    expect(pillText(summarizeDownloads([job]))).toContain("sent to osu!lazer");
    const restarted = applyStateEvent([{ ...job, importMethod: "savedForLazer" }], { setId: 1, state: "queued", error: null });
    expect(restarted[0].importMethod).toBeUndefined(); expect(summarizeDownloads(restarted).lazerManual).toBeUndefined();
  });
});
