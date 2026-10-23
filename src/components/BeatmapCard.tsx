import type { CSSProperties } from "react";
import {
  compactNumber,
  downloadFraction,
  downloadLabel,
  formatLength,
  longestLength,
  statusLabel,
} from "../lib/format";
import { isBusy } from "../lib/jobs";
import { starColor } from "../lib/starColor";
import type { Beatmap, Beatmapset, DownloadJob } from "../lib/types";

const MAX_DOTS = 14;

/** One dot per difficulty in osu!'s star colours, easiest first. */
function DifficultyDots({ beatmaps }: { beatmaps: Beatmap[] }) {
  const sorted = [...beatmaps].sort((a, b) => a.difficulty_rating - b.difficulty_rating);
  const shown = sorted.slice(0, MAX_DOTS);
  const hidden = sorted.length - shown.length;
  return (
    <div className="dots" aria-label={`${beatmaps.length} difficulties`}>
      {shown.map((map) => (
        <span
          key={map.id}
          className="dot"
          style={{ background: starColor(map.difficulty_rating) }}
          title={`${map.version} · ${map.difficulty_rating.toFixed(2)}★ · ${map.mode}`}
        />
      ))}
      {hidden > 0 && <span className="more">+{hidden}</span>}
    </div>
  );
}

function Action(props: {
  installed: boolean;
  job: DownloadJob | undefined;
  onDownload: () => void;
  onCancel: () => void;
}) {
  const { installed, job } = props;
  if (job && isBusy(job)) {
    const fill = `${(downloadFraction(job) ?? 0) * 100}%`;
    return (
      <button className="action busy" style={{ "--fill": fill } as CSSProperties} onClick={props.onCancel}>
        Cancel · {downloadLabel(job)}
      </button>
    );
  }
  if (job?.state === "failed") {
    return (
      <button className="action failed" onClick={props.onDownload} title={job.error ?? undefined}>
        Failed · Retry
      </button>
    );
  }
  if (installed) {
    return (
      <button className="action installed" disabled>
        Installed ✓
      </button>
    );
  }
  if (job?.state === "done") {
    return (
      <button className="action sent" disabled>
        Sent to osu! ✓
      </button>
    );
  }
  return (
    <button className="action" onClick={props.onDownload}>
      Download
    </button>
  );
}

export function BeatmapCard(props: {
  set: Beatmapset;
  index: number;
  installed: boolean;
  job: DownloadJob | undefined;
  selected: boolean;
  playing: boolean;
  /** Leave the cover image out, so it isn't held decoded while the overlay is hidden. */
  releaseCover?: boolean;
  onDownload: () => void;
  onCancel: () => void;
  onPreview: () => void;
  onSelect: () => void;
}) {
  const { set } = props;
  const cover = set.covers["card@2x"] || set.covers.card;
  const classes = ["card", props.selected && "selected", props.installed && "installed"];
  return (
    <li
      className={classes.filter(Boolean).join(" ")}
      data-index={props.index}
      aria-selected={props.selected}
      // Cards fade up one after another; the delay is capped so a long page doesn't crawl in.
      style={{ "--i": Math.min(props.index, 14) } as CSSProperties}
      onMouseDown={props.onSelect}
    >
      <div className="cover">
        {cover && !props.releaseCover && <img src={cover} alt="" loading="lazy" draggable={false} />}
        <span className={`badge status-${set.status}`}>{statusLabel(set.status)}</span>
        <button
          className={`play${props.playing ? " on" : ""}`}
          onClick={props.onPreview}
          aria-label={props.playing ? "Stop preview" : "Play preview"}
        >
          {props.playing ? "❚❚" : "▶"}
        </button>
      </div>
      <div className="body">
        <div className="title" title={set.title}>
          {set.title}
        </div>
        <div className="artist" title={set.artist}>
          {set.artist}
        </div>
        <div className="mapper">mapped by {set.creator}</div>
        <DifficultyDots beatmaps={set.beatmaps} />
        <div className="stats">
          <span title="Length of the longest difficulty">⏱ {formatLength(longestLength(set))}</span>
          <span title="BPM">♪ {Math.round(set.bpm)}</span>
          <span title="Plays">▶ {compactNumber(set.play_count)}</span>
          <span title="Favourites">♥ {compactNumber(set.favourite_count)}</span>
          {set.video && <span title="Has a video">🎞</span>}
          {set.storyboard && <span title="Has a storyboard">🖼</span>}
        </div>
      </div>
      <Action installed={props.installed} job={props.job} onDownload={props.onDownload} onCancel={props.onCancel} />
    </li>
  );
}
