import type { CSSProperties, ReactNode } from "react";
import { downloadFraction, downloadLabel, formatLength, longestLength, starRange } from "../lib/format";
import { isBusy } from "../lib/jobs";
import { starColor } from "../lib/starColor";
import type { Layout } from "../lib/prefs";
import type { Beatmap, Beatmapset, DownloadJob } from "../lib/types";

const MAX_DOTS = 6;

type IconName = "download" | "check" | "retry" | "close" | "play" | "pause" | "film";

const ICONS: Record<IconName, ReactNode> = {
  download: <path d="M12 4.5v10m0 0-4-4m4 4 4-4M5.5 19h13" />,
  check: <path d="m5.5 12.5 4.2 4.2 8.8-9.4" />,
  retry: <path d="M19.5 12a7.5 7.5 0 1 1-2.4-5.5M19.5 4.5V9H15" />,
  close: <path d="m7 7 10 10M17 7 7 17" />,
  play: <path d="M8.5 5.8v12.4L18.5 12z" fill="currentColor" stroke="none" />,
  pause: <path d="M8 6h3v12H8zM13 6h3v12h-3z" fill="currentColor" stroke="none" />,
  film: <path d="M4 5h16v14H4zM8 5v14M16 5v14M4 9.5h4M16 9.5h4M4 14.5h4M16 14.5h4" />,
};

function Icon({ name }: { name: IconName }) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      {ICONS[name]}
    </svg>
  );
}

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

/** The card's one control, a round icon: download, a progress ring that cancels, retry, or a tick. */
function Action(props: {
  installed: boolean;
  job: DownloadJob | undefined;
  onDownload: () => void;
  onCancel: () => void;
}) {
  const { installed, job } = props;
  if (job && isBusy(job)) {
    const fraction = downloadFraction(job);
    const label = downloadLabel(job);
    return (
      <button
        className="action busy"
        style={{ "--fill": `${(fraction ?? 0.25) * 360}deg` } as CSSProperties}
        data-waiting={fraction === null ? "" : undefined}
        onClick={props.onCancel}
        title={`${label} · click to cancel`}
        aria-label={`Cancel download (${label})`}
      >
        <Icon name="close" />
      </button>
    );
  }
  if (job?.state === "failed") {
    return (
      <button
        className="action failed"
        onClick={props.onDownload}
        title={job.error ? `${job.error} · click to try again` : "Failed · click to try again"}
        aria-label="Try the download again"
      >
        <Icon name="retry" />
      </button>
    );
  }
  if (installed || job?.state === "done") {
    const label = installed ? "In your library" : job ? downloadLabel(job) : "Sent to osu!";
    return (
      <span className="action done" title={label} role="img" aria-label={label}>
        <Icon name="check" />
      </span>
    );
  }
  return (
    <button className="action" onClick={props.onDownload} title="Download" aria-label="Download">
      <Icon name="download" />
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
  /** List rows show a small square cover; cards show a wide strip. */
  layout: Layout;
  onDownload: () => void;
  onCancel: () => void;
  onPreview: () => void;
  onSelect: () => void;
}) {
  const { set } = props;
  // A list row shows a small square cover (150x150) and a card a wide strip (400x140): each is plenty on an
  // ordinary screen and far lighter than its 2x file. Only dense screens (above 1.5x) get the sharper one.
  const sharp = window.devicePixelRatio > 1.5;
  const [small, big] = props.layout === "list" ? (["list", "list@2x"] as const) : (["card", "card@2x"] as const);
  const cover = (sharp ? set.covers[big] : set.covers[small]) || set.covers[big] || set.covers[small];
  const stars = starRange(set);
  const classes = ["card", props.selected && "selected", props.installed && "installed"];
  return (
    <li
      className={classes.filter(Boolean).join(" ")}
      data-index={props.index}
      aria-selected={props.selected}
      // Cards fade up one after another; the delay is capped (to about a tenth of a second) so a long
      // page, or the next page of a scroll, shows at once instead of crawling in.
      style={{ "--i": Math.min(props.index, 8) } as CSSProperties}
      onMouseDown={props.onSelect}
    >
      <div className="cover">
        {cover && !props.releaseCover && <img src={cover} alt="" loading="lazy" decoding="async" draggable={false} />}
        {(set.track_id || set.video) && (
          <div className="badges">
            {set.track_id ? (
              <span className="badge" title="Featured Artist">
                Featured
              </span>
            ) : null}
            {set.video && (
              <span className="badge icon" title="Has a video" role="img" aria-label="Has a video">
                <Icon name="film" />
              </span>
            )}
          </div>
        )}
        <button
          className={`play${props.playing ? " on" : ""}`}
          onClick={props.onPreview}
          title={props.playing ? "Stop preview" : "Preview"}
          aria-label={props.playing ? "Stop preview" : "Play preview"}
        >
          <Icon name={props.playing ? "pause" : "play"} />
        </button>
      </div>
      <div className="body">
        <div className="text">
          <div className="title" title={set.title}>
            {set.title}
          </div>
          <div className="artist" title={`${set.artist} · mapped by ${set.creator}`}>
            {set.artist}
            <span className="by"> · mapped by {set.creator}</span>
          </div>
          <div className="meta">
            <DifficultyDots beatmaps={set.beatmaps} />
            <span className="facts" title="Star rating of the easiest and hardest difficulty, length, BPM">
              {stars && `★ ${stars} · `}
              {formatLength(longestLength(set))} · {Math.round(set.bpm)} bpm
            </span>
          </div>
        </div>
        <Action installed={props.installed} job={props.job} onDownload={props.onDownload} onCancel={props.onCancel} />
      </div>
    </li>
  );
}
