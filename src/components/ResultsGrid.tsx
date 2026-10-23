import { useEffect, useRef } from "react";
import type { CSSProperties } from "react";
import type { Beatmapset, DownloadJob } from "../lib/types";
import { BeatmapCard } from "./BeatmapCard";

const MIN_CARD_WIDTH = 280;
const GAP = 12;

interface Props {
  sets: Beatmapset[];
  installed: ReadonlySet<number>;
  jobFor: ReadonlyMap<number, DownloadJob>;
  selected: number;
  playingId: number | null;
  hasMore: boolean;
  loading: boolean;
  error: string | null;
  /** While true the cards show no cover images, so the window doesn't keep them decoded in memory. */
  releaseCovers?: boolean;
  /** Told how many columns fit, so arrow-key navigation matches what is on screen. */
  onColumnsChange: (columns: number) => void;
  onLoadMore: () => void;
  onRetry: () => void;
  onSelect: (index: number) => void;
  onDownload: (set: Beatmapset) => void;
  onCancel: (setId: number) => void;
  onPreview: (set: Beatmapset) => void;
}

export function ResultsGrid(props: Props) {
  const { onColumnsChange, onLoadMore, sets, hasMore, loading, selected } = props;
  const scroller = useRef<HTMLDivElement>(null);
  const sentinel = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const element = scroller.current;
    if (!element) return;
    const observer = new ResizeObserver(() => {
      const style = getComputedStyle(element);
      const inner = element.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
      onColumnsChange(Math.max(1, Math.floor((inner + GAP) / (MIN_CARD_WIDTH + GAP))));
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [onColumnsChange]);

  // Infinite scroll. Re-created whenever the list or loading state changes, so a sentinel that
  // is still on screen after a short page triggers the next page instead of getting stuck.
  useEffect(() => {
    const target = sentinel.current;
    const root = scroller.current;
    if (!target || !root || !hasMore) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) onLoadMore();
      },
      { root, rootMargin: "400px" },
    );
    observer.observe(target);
    return () => observer.disconnect();
  }, [onLoadMore, hasMore, loading, sets.length]);

  // True for the moment the mouse is what selects a card. Selecting with the mouse must never
  // move the list: pressing Download on a card that is only partly in view used to scroll it
  // fully into view, the button slid out from under the cursor, and the click was lost.
  const pointerSelecting = useRef(false);

  useEffect(() => {
    if (selected < 0 || pointerSelecting.current) return;
    scroller.current
      ?.querySelector(`[data-index="${selected}"]`)
      ?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [selected]);

  return (
    <div
      className="results"
      ref={scroller}
      onMouseDownCapture={() => {
        // The selection this causes is handled (and its effect has run) before the timeout fires.
        pointerSelecting.current = true;
        setTimeout(() => {
          pointerSelecting.current = false;
        }, 0);
      }}
    >
      {props.error && (
        <div className="notice error">
          <span>Couldn't load maps: {props.error}</span>
          <button onClick={props.onRetry}>Try again</button>
        </div>
      )}
      {props.loading && sets.length === 0 && !props.error && (
        // Placeholders keep the layout steady (and the window alive) while the first results load.
        <ul className="grid" style={{ "--gap": `${GAP}px`, "--min-card": `${MIN_CARD_WIDTH}px` } as CSSProperties} aria-hidden>
          {Array.from({ length: 6 }, (_, index) => (
            <li key={index} className="card ghost" style={{ "--i": index } as CSSProperties} />
          ))}
        </ul>
      )}
      <ul className="grid" style={{ "--gap": `${GAP}px`, "--min-card": `${MIN_CARD_WIDTH}px` } as CSSProperties}>
        {sets.map((set, index) => (
          <BeatmapCard
            key={set.id}
            set={set}
            index={index}
            installed={props.installed.has(set.id)}
            job={props.jobFor.get(set.id)}
            selected={index === selected}
            playing={props.playingId === set.id}
            releaseCover={props.releaseCovers}
            onDownload={() => props.onDownload(set)}
            onCancel={() => props.onCancel(set.id)}
            onPreview={() => props.onPreview(set)}
            onSelect={() => props.onSelect(index)}
          />
        ))}
      </ul>
      {!props.loading && !props.error && sets.length === 0 && (
        <div className="notice">No maps match. Try fewer words, another status, or all modes.</div>
      )}
      {props.loading && sets.length > 0 && <div className="notice">Loading more…</div>}
      {hasMore && <div className="sentinel" ref={sentinel} aria-hidden />}
    </div>
  );
}
