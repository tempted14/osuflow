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
  const reset = event.state === "queued" ? { received: 0, total: null, mirror: null } : {};
  return upsertJob(jobs, event.setId, { ...reset, state: event.state, error: event.error });
}

export function applyProgressEvent(jobs: DownloadJob[], event: DownloadProgressEvent): DownloadJob[] {
  return upsertJob(jobs, event.setId, {
    received: event.received,
    total: event.total,
    mirror: event.mirror,
  });
}

export const removeJob = (jobs: DownloadJob[], setId: number) => jobs.filter((job) => job.setId !== setId);

export const removeFinished = (jobs: DownloadJob[]) => jobs.filter(isBusy);
