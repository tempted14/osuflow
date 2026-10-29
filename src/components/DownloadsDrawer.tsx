import { useEffect, useState } from "react";
import type { CSSProperties } from "react";
import { downloadFraction, downloadLabel } from "../lib/format";
import { isBusy, pillText, summarizeDownloads } from "../lib/jobs";
import type { DownloadJob } from "../lib/types";

/** Once everything has finished, the pill stays this long and then steps aside until the next download. */
const SETTLE_AFTER_MS = 6000;

/**
 * Downloads, kept small: a pill in the status bar says what is happening ("2 downloading · 45%"),
 * and the list of downloads only opens when the pill is clicked, so it never covers the results.
 */
export function DownloadsDrawer(props: {
  jobs: DownloadJob[];
  onCancel: (setId: number) => void;
  onRetry: (job: DownloadJob) => void;
  onDismiss: (setId: number) => void;
  onClear: () => void;
  onOpenPage: (setId: number) => void;
}) {
  const { jobs } = props;
  const [open, setOpen] = useState(false);
  const [settled, setSettled] = useState(false);
  const summary = summarizeDownloads(jobs);
  const working = summary.active > 0 || summary.failed > 0 || (summary.manual ?? 0) > 0;

  useEffect(() => {
    if (working || open) {
      setSettled(false);
      return;
    }
    const timer = setTimeout(() => setSettled(true), SETTLE_AFTER_MS);
    return () => clearTimeout(timer);
  }, [working, open, jobs.length]);

  // With nothing left to list there is nothing to keep open.
  useEffect(() => {
    if (jobs.length === 0) setOpen(false);
  }, [jobs.length]);

  if (jobs.length === 0 || (settled && !open)) return null;

  const state = summary.active > 0 ? "busy" : summary.failed > 0 ? "failed" : "done";
  const fill = `${(summary.fraction ?? 0) * 100}%`;
  return (
    <aside className="dl" aria-label="Downloads">
      {open && (
        <div className="dl-panel">
          <header>
            <span>Downloads</span>
            <button className="link" onClick={props.onClear}>
              Clear finished
            </button>
          </header>
          <ul>
            {[...jobs].reverse().map((job) => {
              const fraction = downloadFraction(job);
              const mirrorsDown = job.error?.includes("every mirror failed");
              return (
                <li key={job.setId} className={`dl-row ${job.state}`}>
                  <div className="dl-line">
                    <span className="dl-name" title={job.label}>
                      {job.label}
                    </span>
                    <span className="dl-state">{downloadLabel(job)}</span>
                    <span className="dl-actions">
                      {isBusy(job) && (
                        <button className="quiet" title="Cancel" aria-label="Cancel" onClick={() => props.onCancel(job.setId)}>
                          ✕
                        </button>
                      )}
                      {job.state === "failed" && (
                        <button className="quiet" title="Try again" aria-label="Try again" onClick={() => props.onRetry(job)}>
                          ↻
                        </button>
                      )}
                      {mirrorsDown && (
                        <button
                          className="quiet"
                          title="Open on the osu! website"
                          aria-label="Open on the osu! website"
                          onClick={() => props.onOpenPage(job.setId)}
                        >
                          ↗
                        </button>
                      )}
                      {!isBusy(job) && (
                        <button className="quiet" title="Dismiss" aria-label="Dismiss" onClick={() => props.onDismiss(job.setId)}>
                          ✕
                        </button>
                      )}
                    </span>
                  </div>
                  {fraction !== null && isBusy(job) && (
                    <div className="bar" style={{ "--fill": `${fraction * 100}%` } as CSSProperties} />
                  )}
                  {job.state === "failed" && job.error && (
                    <div className="dl-error" title={job.error}>
                      {job.error}
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
        </div>
      )}
      <button
        className={`dl-pill ${state}`}
        style={{ "--fill": fill } as CSSProperties}
        onClick={() => setOpen(!open)}
        aria-expanded={open}
      >
        {pillText(summary)}
      </button>
    </aside>
  );
}
