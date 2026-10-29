/** The parts of a KeyboardEvent needed to describe a shortcut. */
export interface KeyChord {
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

const MODIFIER_CODES = /^(Control|Shift|Alt|Meta|OS)(Left|Right)?$/;

/**
 * "Ctrl+Shift+D" for the keys currently held, in the spelling the backend accepts. Returns null
 * while only modifiers are down, so the settings box waits for the actual key.
 */
export function hotkeyFromEvent(event: KeyChord, platform: import("./platform").Platform = "windows"): string | null {
  if (MODIFIER_CODES.test(event.code)) return null;
  const key = event.code.replace(/^Key(?=[A-Z]$)/, "").replace(/^Digit(?=\d$)/, "");
  const parts = [
    event.ctrlKey && "Ctrl",
    event.altKey && "Alt",
    event.shiftKey && "Shift",
    event.metaKey && (platform === "macos" ? "Command" : "Super"),
    key,
  ];
  return parts.filter(Boolean).join("+");
}
