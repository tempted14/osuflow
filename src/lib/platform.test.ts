import { describe, expect, it } from "vitest";
import { displayHotkey } from "./platform";
import { hotkeyFromEvent } from "./hotkey";
import { downloadLabel } from "./format";

describe("platform behavior", () => {
  it("shows and captures the Command key on macOS without changing Windows shortcuts", () => {
    expect(displayHotkey("CommandOrControl+Shift+D", "macos")).toBe("Command+Shift+D");
    expect(displayHotkey("CommandOrControl+Shift+D", "linux")).toBe("Ctrl+Shift+D");
    expect(displayHotkey("Ctrl+Shift+D", "windows")).toBe("Ctrl+Shift+D");
    const chord = { code: "KeyD", metaKey: true, ctrlKey: false, altKey: false, shiftKey: true };
    expect(hotkeyFromEvent(chord, "macos")).toBe("Shift+Command+D");
    expect(hotkeyFromEvent(chord)).toBe("Shift+Super+D");
  });
  it("explains the Songs-folder fallback instead of claiming an automatic import", () => {
    expect(downloadLabel({ state: "done", received: 1, total: 1, importMethod: "movedToSongs" })).toContain("F5");
    expect(downloadLabel({ state: "done", received: 1, total: 1 })).toBe("Sent to osu!");
  });
});
