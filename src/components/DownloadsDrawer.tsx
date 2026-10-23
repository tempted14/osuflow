import { useEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import { downloadFraction, downloadLabel } from "../lib/format";
import { isBusy } from "../lib/jobs";
import type { DownloadJob } from "../lib/types";

/** Downloads in progress and recently finished, collapsible, in the corner of the overlay. */
export function DownloadsDrawer(props: {
  jobs: DownloadJob[];
  onCancel: (setId: number) => void;
  onRetry: (job: DownloadJob) => void;
  onDismiss: (setId: number) => void;
  onClear: () => void;
  onOpenPage: (setId: number) => void;
}) {
  const { jobs } = props;
  const [open, setOpen] = useState(true);
  const previousCount = useRef(0);

  // A new download pops the drawer open so progress is visible.
  useEffect(() => {
    if (jobs.length > previousCount.current) setOpen(true);
    previousCount.current = jobs.length;
  }, [jobs.length]);

  if (jobs.length === 0) return null;
  const active = jobs.filter(isBusy).length;
  return (
    <aside className="drawer" aria-label="Downloads">
      <header>
        <button className="drawer-toggle" onClick={() => setOpen(!open)} aria-expanded={open}>
          Downloads{active > 0 ? ` · ${active} active` : ""} <span aria-hidden>{open ? "▾" : "▴"}</span>
        </button>
        <button className="link" onClick={props.onClear}>
          Clear finished
        </button>
      </header>
      {open && (
        <ul>
          {[...jobs].reverse().map((job) => {
            const fraction = downloadFraction(job);
            const mirrorsDown = job.error?.includes("every mirror failed");
            return (
              <li key={job.setId} className={`job ${job.state}`}>
                <div className="job-line">
                  <span className="job-name" title={job.label}>
                    {job.label}
                  </span>
                  <span className="job-state">{downloadLabel(job)}</span>
                </div>
                {fraction !== null && isBusy(job) && (
                  <div className="bar" style={{ "--fill": `${fraction * 100}%` } as CSSProperties} />
                )}
                {job.state === "failed" && job.error && <div className="job-error">{job.error}</div>}
                <div className="job-actions">
                  {isBusy(job) && <button onClick={() => props.onCancel(job.setId)}>Cancel</button>}
                  {job.state === "failed" && <button onClick={() => props.onRetry(job)}>Retry</button>}
                  {mirrorsDown && (
                    <button onClick={() => props.onOpenPage(job.setId)}>Open on osu! website</button>
                  )}
                  {!isBusy(job) && (
                    <button className="quiet" onClick={() => props.onDismiss(job.setId)} aria-label="Dismiss">
                      ✕
                    </button>
                  )}
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </aside>
  );
}
