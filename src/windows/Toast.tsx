import { useEffect, useState } from "react";
import type { CSSProperties } from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "../lib/api";
import { autoHideMs, toastLine, toastProgress } from "../lib/links";
import type { DownloadProgressEvent, DownloadStateEvent, LinkToast } from "../lib/types";

/**
 * The small corner window that appears when a beatmap link is clicked. It is never given
 * keyboard focus (the window isn't focusable), so it can appear over a game without
 * interrupting it.
 */
export default function Toast() {
  const [toast, setToast] = useState<LinkToast | null>(null);
  const [stage, setStage] = useState<DownloadStateEvent | null>(null);
  const [progress, setProgress] = useState<DownloadProgressEvent | null>(null);

  useEffect(() => {
    const listeners = Promise.all([
      listen<LinkToast>("link-toast", ({ payload }) => {
        setToast(payload);
        // A different map starts with a clean slate.
        setStage((old) => (old && old.setId === payload.setId ? old : null));
        setProgress((old) => (old && old.setId === payload.setId ? old : null));
      }),
      listen<DownloadStateEvent>("download-state", ({ payload }) => setStage(payload)),
      listen<DownloadProgressEvent>("download-progress", ({ payload }) => setProgress(payload)),
    ]);
    return () => {
      void listeners.then((stops) => stops.forEach((stop) => stop()));
    };
  }, []);

  useEffect(() => {
    if (!toast) return;
    const delay = autoHideMs(toast.state);
    if (delay === null) return;
    const timer = setTimeout(() => void api.dismissToast(), delay);
    return () => clearTimeout(timer);
  }, [toast]);

  if (!toast || toast.state === "hidden") return null;
  const fraction = toastProgress(toast, progress);
  const failed = toast.state === "failed";

  return (
    <div className={`toast ${toast.state}`} role="status">
      <div className="toast-head">
        <strong className="toast-title" title={toast.label}>
          {toast.label || "Beatmap link"}
        </strong>
        <button className="quiet" onClick={() => void api.dismissToast()} aria-label="Dismiss">
          ✕
        </button>
      </div>
      <div className="toast-line">{toastLine(toast, stage, progress)}</div>
      {fraction !== null && <div className="bar" style={{ "--fill": `${fraction * 100}%` } as CSSProperties} />}
      <div className="toast-actions">
        {failed && <button onClick={() => void api.toastRetry(toast.url)}>Try again</button>}
        <button
          onClick={() => {
            void api.toastOpenPage(toast.url);
            void api.dismissToast();
          }}
        >
          Open page instead
        </button>
      </div>
    </div>
  );
}
