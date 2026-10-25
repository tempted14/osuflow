import type { ChangeEvent, CSSProperties, PointerEvent } from "react";
import { NO_SPAN, handlesOf, spanActive, spanFromHandles, spanLabel } from "../lib/ranges";
import type { SliderDef, Span } from "../lib/ranges";

/** The handle's width in pixels, which the groove and a click on it are measured against. */
const THUMB = 16;

/**
 * A slider with a handle at each end of the range you want, like the star-rating slider in a game's song
 * list. A handle left on the end of the track means no limit on that side, so a slider at full width asks
 * for nothing. It is two ordinary range inputs laid over one track, so the keyboard (arrow keys, Home, End)
 * works on whichever handle has focus.
 */
export function RangeSlider(props: { def: SliderDef; span: Span; onChange: (span: Span) => void }) {
  const { def, span } = props;
  const [low, high] = handlesOf(def, span);
  const percent = (value: number) => ((value - def.min) / (def.max - def.min)) * 100;
  const active = spanActive(span);

  // The handles can meet, which asks for exactly that value, but never cross.
  const moveLow = (event: ChangeEvent<HTMLInputElement>) =>
    props.onChange(spanFromHandles(def, Math.min(Number(event.currentTarget.value), high), high));
  const moveHigh = (event: ChangeEvent<HTMLInputElement>) =>
    props.onChange(spanFromHandles(def, low, Math.max(Number(event.currentTarget.value), low)));

  // A click on the groove itself (the handles take their own) sends the nearer handle there.
  const clickTrack = (event: PointerEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget) return;
    const box = event.currentTarget.getBoundingClientRect();
    const fraction = (event.clientX - box.left - THUMB / 2) / (box.width - THUMB);
    const raw = def.min + Math.min(Math.max(fraction, 0), 1) * (def.max - def.min);
    const value = Number((Math.round((raw - def.min) / def.step) * def.step + def.min).toFixed(4));
    // Between the handles it goes to whichever is closer; with both on one spot, to the side it was clicked on.
    const nearLow = Math.abs(value - low) < Math.abs(value - high) || (low === high && value < low);
    props.onChange(nearLow ? spanFromHandles(def, value, high) : spanFromHandles(def, low, value));
  };

  return (
    <div className={`slider${active ? " on" : ""}`} title={def.hint}>
      <div className="slider-head">
        <span className="slider-label">{def.label}</span>
        {active ? (
          <button
            className="slider-value"
            onClick={() => props.onChange(NO_SPAN)}
            title={`Clear ${def.label}`}
            aria-label={`${def.label}: ${spanLabel(def, span)}. Clear`}
          >
            {spanLabel(def, span)}
            <svg width="9" height="9" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" aria-hidden>
              <path d="m6 6 12 12M18 6 6 18" />
            </svg>
          </button>
        ) : (
          <span className="slider-value idle">Any</span>
        )}
      </div>
      <div
        className="slider-track"
        style={{ "--lo": percent(low), "--hi": percent(high), "--thumb": `${THUMB}px` } as CSSProperties}
        onPointerDown={clickTrack}
      >
        <input
          type="range"
          min={def.min}
          max={def.max}
          step={def.step}
          value={low}
          onChange={moveLow}
          aria-label={`${def.label}, lowest`}
          aria-valuetext={def.format(low)}
          // When both handles sit at the far end the lower one has to be the one on top, or it can't be grabbed.
          style={{ zIndex: percent(low) > 50 ? 2 : 1 }}
        />
        <input
          type="range"
          min={def.min}
          max={def.max}
          step={def.step}
          value={high}
          onChange={moveHigh}
          aria-label={`${def.label}, highest`}
          aria-valuetext={def.format(high)}
        />
      </div>
    </div>
  );
}
