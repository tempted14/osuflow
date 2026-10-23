import type {
  DownloadProgressEvent,
  DownloadStateEvent,
  LinkHandlerStatus,
  LinkToast,
  ToastState,
} from "./types";

/** How long a toast stays on screen by itself; null means until the user dismisses it. */
export function autoHideMs(state: ToastState): number | null {
  switch (state) {
    case "installed":
    case "done":
      return 4000;
    case "failed":
      return 15000;
    default:
      return null;
  }
}

/** 0..1 while this toast's own map is downloading, otherwise null. */
export function toastProgress(toast: LinkToast, progress: DownloadProgressEvent | null): number | null {
  if (toast.state !== "working" || !progress || progress.setId !== toast.setId || !progress.total) return null;
  return Math.min(1, progress.received / progress.total);
}

/**
 * The line under the title. While a map is being fetched the backend's one-off "Downloading…"
 * is replaced by what the download is doing right now, taken from the `download-*` events.
 */
export function toastLine(
  toast: LinkToast,
  stage: DownloadStateEvent | null,
  progress: DownloadProgressEvent | null,
): string {
  if (toast.state === "working" && stage && stage.setId === toast.setId) {
    if (stage.state === "importing") return "Sending to osu!…";
    if (stage.state === "downloading") {
      const fraction = toastProgress(toast, progress);
      return fraction === null ? "Downloading…" : `Downloading… ${Math.round(fraction * 100)}%`;
    }
    if (stage.state === "queued") return "Waiting for another download…";
  }
  return toast.message ?? "";
}

/** What the "Beatmap links" section should show. */
export type LinkMode =
  /** Switched off and not the default browser: nothing is happening. */
  | "off"
  /** Switched on, but the user hasn't chosen QuickDirect in Default apps yet. */
  | "waiting"
  /** Switched on and chosen: beatmap links are downloaded. */
  | "active"
  /** Switched off, but Windows still sends links here, so they're all passed on to the browser. */
  | "forwarding";

export function linkMode(status: LinkHandlerStatus): LinkMode {
  if (status.enabled) return status.isDefault ? "active" : "waiting";
  return status.isDefault ? "forwarding" : "off";
}

const BROWSER_NAMES: Record<string, string> = {
  "chrome.exe": "Chrome",
  "msedge.exe": "Edge",
  "firefox.exe": "Firefox",
  "brave.exe": "Brave",
  "opera.exe": "Opera",
  "vivaldi.exe": "Vivaldi",
};

/** "C:\...\chrome.exe" -> "Chrome"; unknown programs keep their file name. */
export function browserName(path: string | null): string {
  if (!path) return "your usual browser";
  const file = path.split(/[\\/]/).pop()?.toLowerCase() ?? "";
  return BROWSER_NAMES[file] ?? (file.replace(/\.exe$/, "") || "your usual browser");
}
