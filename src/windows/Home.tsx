import { useEffect, useState } from "react";
import type { KeyboardEvent } from "react";
import * as api from "../lib/api";
import { LinkSettings } from "../components/LinkSettings";
import { useSettings } from "../hooks/useSettings";
import { hotkeyFromEvent } from "../lib/hotkey";
import type { HotkeyStatus, OsuInstall } from "../lib/types";

const DEFAULT_HOTKEY = "Ctrl+Shift+D";

const SOURCE: Record<OsuInstall["source"], string> = {
  settings: "from your settings",
  runningProcess: "running right now",
  registry: "found through the .osz file association",
  defaultPath: "in the default folder",
};

/** The settings window: shows what QuickDirect found and lets the user change the hotkey. */
export default function Home() {
  const { settings, update } = useSettings();
  const [install, setInstall] = useState<OsuInstall | null | undefined>(undefined);
  const [status, setStatus] = useState<HotkeyStatus | null>(null);
  const [pending, setPending] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    api.detectOsu().then(setInstall).catch(() => setInstall(null));
    api.getHotkeyStatus().then(setStatus).catch(() => undefined);
  }, []);

  const shown = pending ?? status?.hotkey ?? "";
  const dirty = pending !== null && pending !== status?.hotkey;

  const capture = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Tab") return; // keep keyboard navigation working
    event.preventDefault();
    const chord = hotkeyFromEvent(event.nativeEvent);
    if (chord) {
      setPending(chord);
      setError(null);
    }
  };

  const save = async () => {
    if (!pending) return;
    setSaving(true);
    setError(null);
    try {
      setStatus(await api.setHotkey(pending));
      setPending(null);
    } catch (failure) {
      setError(String(failure));
      api.getHotkeyStatus().then(setStatus).catch(() => undefined);
    } finally {
      setSaving(false);
    }
  };

  return (
    <main className="home">
      <h1>QuickDirect</h1>
      <p className="lead">
        Press <kbd>{status?.hotkey ?? "…"}</kbd> in osu! to search for maps and download them without leaving the
        game.
      </p>
      <button className="primary" onClick={() => void api.toggleOverlay()}>
        Open the search now
      </button>

      <section>
        <h2>osu!</h2>
        {install === undefined && <p className="muted">Looking for osu!…</p>}
        {install === null && (
          <p className="warn">
            osu! stable wasn't found. Maps are still saved, but they can't be imported until it's installed.
          </p>
        )}
        {install && (
          <>
            <p>
              Found <code>{install.exe}</code> <span className="muted">({SOURCE[install.source]})</span>
            </p>
            <p className="muted">
              Maps go into <code>{install.songsDir}</code>
            </p>
            {install.exclusiveFullscreen && (
              <p className="warn">
                osu! is set to exclusive fullscreen, so the overlay can't draw over it. Switch to borderless or
                windowed in osu!'s Options → Graphics.
              </p>
            )}
          </>
        )}
      </section>

      <section>
        <h2>Shortcut</h2>
        <div className="hotkey-row">
          <input
            className="hotkey"
            readOnly
            value={shown}
            onKeyDown={capture}
            aria-label="Shortcut. Click here and press the new keys."
          />
          <button onClick={() => void save()} disabled={!dirty || saving}>
            Save
          </button>
          <button onClick={() => setPending(DEFAULT_HOTKEY)} disabled={shown === DEFAULT_HOTKEY}>
            Default
          </button>
        </div>
        <p className="muted">Click the box and press the keys you want, for example Ctrl+Alt+M.</p>
        {error && <p className="warn">{error}</p>}
        {status && !status.registered && !error && (
          <p className="warn">
            {status.error ?? "Windows didn't accept this shortcut."} Pick another one and save it.
          </p>
        )}
      </section>

      <section>
        <h2>Downloads</h2>
        <label className="toggle">
          <input
            type="checkbox"
            checked={settings?.noVideo ?? true}
            disabled={!settings}
            onChange={(e) => void update({ noVideo: e.currentTarget.checked })}
          />
          Skip videos (smaller, faster downloads)
        </label>
      </section>

      <LinkSettings />

      <p className="foot">
        Closing this window keeps QuickDirect running in the tray. It isn't affiliated with or endorsed by ppy.
      </p>
    </main>
  );
}
