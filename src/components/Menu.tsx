import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { MENU_NAV_KEYS, stepIndex, typeAhead } from "../lib/menu";

export interface MenuOption<T> {
  value: T;
  label: string;
}

/**
 * A drop-down in the style of Windows 11's combo box: the name and the current choice on one button,
 * and a flyout list under it. It stands in for the browser's own <select>, whose pop-up can't be styled.
 * The list takes keyboard focus while it is open, so its keys (arrows, Home, End, Enter, Esc, letters)
 * never reach the results behind it.
 */
export function Menu<T extends string | number>(props: {
  label: string;
  options: readonly MenuOption<T>[];
  value: T;
  onChange: (value: T) => void;
}) {
  const { label, options, value } = props;
  const [open, setOpen] = useState(false);
  const [hot, setHot] = useState(0);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const typed = useRef({ text: "", at: 0 });
  const id = useId();
  const selected = Math.max(0, options.findIndex((option) => option.value === value));

  const show = () => {
    setHot(selected);
    setOpen(true);
  };
  const close = (refocus: boolean) => {
    setOpen(false);
    if (refocus) trigger.current?.focus();
  };
  const choose = (index: number) => {
    props.onChange(options[index].value);
    close(true);
  };

  // If the list would run past the window's right edge, it lines up with its button's right edge instead.
  const [alignEnd, setAlignEnd] = useState(false);
  useLayoutEffect(() => {
    const box = open ? list.current?.getBoundingClientRect() : undefined;
    setAlignEnd(box !== undefined && box.right > window.innerWidth - 8);
  }, [open]);

  useEffect(() => {
    if (open) list.current?.focus({ preventScroll: true });
  }, [open]);

  // A click anywhere else closes it.
  useEffect(() => {
    if (!open) return;
    const away = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", away, true);
    return () => document.removeEventListener("pointerdown", away, true);
  }, [open]);

  useEffect(() => {
    if (open) list.current?.children[hot]?.scrollIntoView({ block: "nearest" });
  }, [open, hot]);

  const onListKey = (event: KeyboardEvent<HTMLUListElement>) => {
    const { key } = event;
    const handled = () => {
      event.preventDefault();
      event.stopPropagation();
    };
    if (key === "Escape") {
      handled();
      close(true);
    } else if (key === "Enter" || key === " ") {
      handled();
      choose(hot);
    } else if (key === "Tab") {
      setOpen(false);
    } else if (MENU_NAV_KEYS.includes(key)) {
      handled();
      setHot((current) => stepIndex(current, key, options.length));
    } else if (key === "ArrowLeft" || key === "ArrowRight") {
      handled();
    } else if (key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
      event.stopPropagation();
      const now = performance.now();
      const text = now - typed.current.at > 700 ? key : typed.current.text + key;
      typed.current = { text, at: now };
      const found = typeAhead(options.map((option) => option.label), text, hot);
      if (found >= 0) setHot(found);
    }
  };

  const onTriggerKey = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      event.stopPropagation();
      show();
    }
  };

  return (
    <div className={`menu${open ? " open" : ""}`} ref={root}>
      <button
        ref={trigger}
        className="menu-trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        onClick={() => (open ? close(false) : show())}
        onKeyDown={onTriggerKey}
      >
        <span className="menu-label">{label}</span>
        <span className="menu-value">{options[selected]?.label}</span>
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
          <path d="m6 9 6 6 6-6" />
        </svg>
      </button>
      {open && (
        <ul
          ref={list}
          id={id}
          className={`flyout${alignEnd ? " end" : ""}`}
          role="listbox"
          tabIndex={-1}
          aria-label={label}
          aria-activedescendant={`${id}-${hot}`}
          onKeyDown={onListKey}
        >
          {options.map((option, index) => (
            <li
              key={String(option.value)}
              id={`${id}-${index}`}
              role="option"
              aria-selected={index === selected}
              className={`flyout-item${index === hot ? " hot" : ""}${index === selected ? " on" : ""}`}
              onPointerMove={() => setHot(index)}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => choose(index)}
            >
              {option.label}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
