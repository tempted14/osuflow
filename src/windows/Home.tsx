import { useEffect, useState } from "react";
import type { KeyboardEvent, ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "../lib/api";
import { LinkSettings } from "../components/LinkSettings";
import { Logo } from "../components/Logo";
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

/** One switch with its explanation; the whole row toggles it. */
function SettingRow(props: { title: string; hint?: string; children: ReactNode }) {
  return (
    <label className="setting">
      <span className="setting-text">
        <strong>{props.title}</strong>
        {props.hint && <small>{props.hint}</small>}
      </span>
      {props.children}
    </label>
  );
}

/** The settings window: shows what osu!flow found and holds its few switches. */
export default function Home() {
  const { settings, update } = useSettings();
  const [install, setInstall] = useState<OsuInstall | null | undefined>(undefined);
  const [status, setStatus] = useState<HotkeyStatus | null>(null);
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [pending, setPending] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [autostartError, setAutostartError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    api.detectOsu().then(setInstall).catch(() => setInstall(null));
    api.getHotkeyStatus().then(setStatus).catch(() => undefined);
    api.getAutostart().then(setAutostart).catch(() => setAutostart(false));
    // The tray menu can switch it too.
    const unlisten = listen<boolean>("autostart-changed", ({ payload }) => setAutostart(payload));
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, []);

  const toggleAutostart = async (enabled: boolean) => {
    setAutostart(enabled); // optimistic, so the switch feels instant
    setAutostartError(null);
    try {
      setAutostart(await api.setAutostart(enabled));
    } catch (failure) {
      setAutostart(!enabled);
      setAutostartError(String(failure));
    }
  };

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
      <header className="hero">
        <Logo size={56} />
        <div>
          <h1>osu!flow</h1>
          <p className="lead">
            Press <kbd>{status?.hotkey ?? "…"}</kbd> in osu! to search for maps and send them to the game without
            leaving it.
          </p>
        </div>
      </header>
      <button className="primary" onClick={() => void api.toggleOverlay()}>
        Open the search now
      </button>

      <section className="group">
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

      <section className="group">
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

      <section className="group">
        <h2>Downloads</h2>
        <SettingRow title="Skip videos" hint="Smaller, faster downloads">
          <input
            type="checkbox"
            checked={settings?.noVideo ?? true}
            disabled={!settings}
            onChange={(e) => void update({ noVideo: e.currentTarget.checked })}
          />
        </SettingRow>
      </section>

      <section className="group">
        <h2>Background</h2>
        <SettingRow title="Start with Windows" hint="Waits quietly in the tray, ready for your shortcut">
          <input
            type="checkbox"
            checked={autostart ?? false}
            disabled={autostart === null}
            onChange={(e) => void toggleAutostart(e.currentTarget.checked)}
          />
        </SettingRow>
        {autostartError && <p className="warn">{autostartError}</p>}
        <p className="muted">Closing this window keeps osu!flow running in the tray.</p>
      </section>

      <LinkSettings />

      <p className="foot">osu!flow isn't affiliated with or endorsed by ppy.</p>
    </main>
  );
}
