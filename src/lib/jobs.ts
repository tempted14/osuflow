import { downloadFraction } from "./format";
import type {
  DownloadJob,
  DownloadProgressEvent,
  DownloadState,
  DownloadStateEvent,
} from "./types";

const BUSY: readonly DownloadState[] = ["queued", "downloading", "importing"];

/** True while a download still has work to do (so it can be cancelled, not retried). */
export const isBusy = (job: Pick<DownloadJob, "state">) => BUSY.includes(job.state);

export function newJob(setId: number, label = `Beatmapset ${setId}`): DownloadJob {
  return { setId, label, state: "queued", received: 0, total: null, error: null, mirror: null };
}

/** Updates the job for `setId`, creating it (at the end) if it doesn't exist yet. */
export function upsertJob(jobs: DownloadJob[], setId: number, patch: Partial<DownloadJob>): DownloadJob[] {
  const index = jobs.findIndex((job) => job.setId === setId);
  if (index === -1) return [...jobs, { ...newJob(setId), ...patch }];
  const next = jobs.slice();
  next[index] = { ...jobs[index], ...patch };
  return next;
}

/** A fresh `queued` event starts a new attempt, so progress from an earlier one is cleared. */
export function applyStateEvent(jobs: DownloadJob[], event: DownloadStateEvent): DownloadJob[] {
  const reset = event.state === "queued" ? { received: 0, total: null, mirror: null, importMethod: undefined } : {};
  return upsertJob(jobs, event.setId, { ...reset, state: event.state, error: event.error });
}

export function applyProgressEvent(jobs: DownloadJob[], event: DownloadProgressEvent): DownloadJob[] {
  return upsertJob(jobs, event.setId, {
    received: event.received,
    total: event.total,
    mirror: event.mirror,
  });
}

/** What the downloads pill needs to know, at a glance. */
export interface DownloadSummary {
  /** Downloads waiting for an F5 refresh in song select. */
  manual?: number;
  active: number;
  failed: number;
  done: number;
  /** Overall progress of the running downloads, 0..1, or null until one has reported its size. */
  fraction: number | null;
}

export function summarizeDownloads(jobs: DownloadJob[]): DownloadSummary {
  const running = jobs.filter(isBusy);
  const known = running.map(downloadFraction).filter((fraction): fraction is number => fraction !== null);
  return {
    ...(jobs.some((job) => job.state === "done" && job.importMethod === "movedToSongs") ? { manual: jobs.filter((job) => job.state === "done" && job.importMethod === "movedToSongs").length } : {}),
    active: running.length,
    failed: jobs.filter((job) => job.state === "failed").length,
    done: jobs.filter((job) => job.state === "done").length,
    // Downloads that haven't reported a size yet count as nothing done, so the bar never jumps back.
    fraction: known.length === 0 ? null : known.reduce((sum, fraction) => sum + fraction, 0) / running.length,
  };
}

/** The pill's one line: what is happening now, else what went wrong, else what finished. */
export function pillText(summary: DownloadSummary): string {
  if (summary.active > 0) {
    const what = summary.active === 1 ? "Downloading" : `${summary.active} downloading`;
    return summary.fraction === null ? what : `${what} · ${Math.round(summary.fraction * 100)}%`;
  }
  if (summary.failed > 0) return summary.failed === 1 ? "1 failed" : `${summary.failed} failed`;
  if (summary.manual) return `${summary.manual} saved to Songs — press F5`;
  if (summary.done > 0) return `${summary.done} sent to osu! ✓`;
  return "";
}

export const removeJob = (jobs: DownloadJob[], setId: number) => jobs.filter((job) => job.setId !== setId);

export const removeFinished = (jobs: DownloadJob[]) => jobs.filter(isBusy);
