import type { Beatmapset, DownloadJob, Mirror } from "./types";

/** 187 -> "3:07" */
export function formatLength(seconds: number): string {
  const total = Math.max(0, Math.round(seconds));
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

const COMPACT = new Intl.NumberFormat("en", { notation: "compact", maximumFractionDigits: 1 });

/** 1234 -> "1.2K", 2_500_000 -> "2.5M" */
export function compactNumber(n: number): string {
  return COMPACT.format(n);
}

/** "ranked" -> "Ranked", "wip" -> "WIP" */
export function statusLabel(status: string): string {
  if (status === "wip") return "WIP";
  return status.charAt(0).toUpperCase() + status.slice(1);
}

export function setLabel(set: Pick<Beatmapset, "artist" | "title">): string {
  return `${set.artist} – ${set.title}`;
}

/** The longest difficulty's length in seconds, which is what osu! shows for a set. */
export function longestLength(set: Pick<Beatmapset, "beatmaps">): number {
  return set.beatmaps.reduce((longest, map) => Math.max(longest, map.total_length), 0);
}

export function mirrorLabel(mirror: Mirror): string {
  return mirror === "osuDirect" ? "osu.direct" : "catboy.best";
}

const megabytes = (bytes: number) => (bytes / 1024 / 1024).toFixed(1);

export function downloadLabel(job: Pick<DownloadJob, "state" | "received" | "total">): string {
  switch (job.state) {
    case "queued":
      return "Queued";
    case "downloading":
      return job.total
        ? `${Math.round((job.received / job.total) * 100)}%`
        : `${megabytes(job.received)} MB`;
    case "importing":
      return "Sending to osu!…";
    case "done":
      return "Sent to osu!";
    case "failed":
      return "Failed";
    case "cancelled":
      return "Cancelled";
  }
}

/** 0..1 for a progress bar, or null when there is nothing meaningful to show. */
export function downloadFraction(job: Pick<DownloadJob, "state" | "received" | "total">): number | null {
  if (job.state === "done" || job.state === "importing") return 1;
  if (job.state === "downloading" && job.total) return Math.min(1, job.received / job.total);
  return null;
}
