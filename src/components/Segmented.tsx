import { useLayoutEffect, useRef, useState } from "react";

/**
 * A row of choices where one is picked, with a highlight that glides to the new choice, like the
 * segmented controls of Windows 11. The highlight is measured from the buttons themselves, so labels
 * of any width line up, and it stays put (and doesn't animate in from nowhere) on the first paint.
 */
export function Segmented<T extends string | number | null>(props: {
  label: string;
  options: readonly { value: T; label: string }[];
  value: T;
  onChange: (value: T) => void;
}) {
  const { options, value } = props;
  const group = useRef<HTMLDivElement>(null);
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);
  const [box, setBox] = useState<{ x: number; width: number } | null>(null);
  const [ready, setReady] = useState(false);
  const index = options.findIndex((option) => option.value === value);

  useLayoutEffect(() => {
    const measure = () => {
      const target = buttons.current[index];
      if (target) setBox({ x: target.offsetLeft, width: target.offsetWidth });
    };
    measure();
    // Fonts arriving or the window resizing move the buttons: measure again.
    const observer = new ResizeObserver(measure);
    if (group.current) observer.observe(group.current);
    const frame = requestAnimationFrame(() => setReady(true));
    return () => {
      observer.disconnect();
      cancelAnimationFrame(frame);
    };
  }, [index]);

  return (
    <div className={`seg${ready ? " ready" : ""}`} role="group" aria-label={props.label} ref={group}>
      <span
        className="seg-pill"
        style={box ? { width: box.width, transform: `translateX(${box.x}px)` } : { opacity: 0 }}
        aria-hidden
      />
      {options.map((option, position) => (
        <button
          key={String(option.value)}
          ref={(element) => {
            buttons.current[position] = element;
          }}
          className={position === index ? "on" : ""}
          aria-pressed={position === index}
          onClick={() => props.onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}
