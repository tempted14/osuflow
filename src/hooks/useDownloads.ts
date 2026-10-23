import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "../lib/api";
import {
  applyProgressEvent,
  applyStateEvent,
  isBusy,
  removeFinished,
  removeJob,
  upsertJob,
} from "../lib/jobs";
import type { DownloadJob, DownloadProgressEvent, DownloadStateEvent } from "../lib/types";

/** Every download this window knows about, driven by the backend's `download-*` events. */
export function useDownloads() {
  const [jobs, setJobs] = useState<DownloadJob[]>([]);
  const latest = useRef(jobs);
  latest.current = jobs;

  useEffect(() => {
    const listeners = Promise.all([
      listen<DownloadStateEvent>("download-state", ({ payload }) =>
        setJobs((all) => applyStateEvent(all, payload)),
      ),
      listen<DownloadProgressEvent>("download-progress", ({ payload }) =>
        setJobs((all) => applyProgressEvent(all, payload)),
      ),
    ]);
    return () => {
      void listeners.then((stops) => stops.forEach((stop) => stop()));
    };
  }, []);

  const start = useCallback((setId: number, label: string) => {
    // The backend refuses a second download of the same set, so don't ask while one is running.
    if (latest.current.some((job) => job.setId === setId && isBusy(job))) return;
    setJobs((all) =>
      upsertJob(all, setId, { label, state: "queued", received: 0, total: null, error: null, mirror: null }),
    );
    api.downloadBeatmapset(setId).catch((error) => {
      const message = String(error);
      const cancelled = message.includes("cancelled");
      setJobs((all) =>
        upsertJob(all, setId, cancelled ? { state: "cancelled", error: null } : { state: "failed", error: message }),
      );
    });
  }, []);

  const cancel = useCallback((setId: number) => {
    void api.cancelDownload(setId);
  }, []);
  const dismiss = useCallback((setId: number) => setJobs((all) => removeJob(all, setId)), []);
  const clearFinished = useCallback(() => setJobs(removeFinished), []);

  const jobFor = useMemo(() => new Map(jobs.map((job) => [job.setId, job])), [jobs]);

  return { jobs, jobFor, start, cancel, dismiss, clearFinished };
}
