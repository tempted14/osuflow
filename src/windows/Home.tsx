import { useEffect, useState } from "react";
import type { KeyboardEvent } from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "../lib/api";
import { LinkSettings } from "../components/LinkSettings";
import { Logo } from "../components/Logo";
import { useSettings } from "../hooks/useSettings";
import { hotkeyFromEvent } from "../lib/hotkey";
import type { HotkeyStatus, OsuInstall } from "../lib/types";

const DEFAULT_HOTKEY = "Ctrl+Shift+D";

/** One labelled switch; the whole row toggles it. */
function SwitchRow(props: { title: string; checked: boolean; disabled?: boolean; onChange: (on: boolean) => void }) {
  return (
    <label className="setting">
      <strong>{props.title}</strong>
      <input
        type="checkbox"
        checked={props.checked}
        disabled={props.disabled}
        onChange={(e) => props.onChange(e.currentTarget.checked)}
      />
    </label>
  );
}

/** The settings window: a few flat rows, nothing more. */
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
        <Logo size={30} />
        <h1>osu!flow</h1>
        <span className="version">{__APP_VERSION__}</span>
        <button className="quiet" onClick={() => void api.toggleOverlay()}>
          Open search
        </button>
      </header>

      <div className="setting">
        <div className="setting-text">
          <strong>osu!</strong>
          {install === undefined && <small>Looking…</small>}
          {install === null && (
            <small className="warn">Not found. Maps are saved, but can't be imported until osu! is installed.</small>
          )}
          {install && (
            <small className="path" title={`${install.exe}\nMaps go into ${install.songsDir}`}>
              {install.dir}
            </small>
          )}
          {install?.exclusiveFullscreen && (
            <small className="warn">Exclusive fullscreen hides the overlay. Use borderless or windowed.</small>
          )}
        </div>
        {install && <span className="status-dot" role="img" aria-label="osu! found" />}
      </div>

      <div className="setting">
        <strong>Shortcut</strong>
        <div className="actions">
          {shown !== DEFAULT_HOTKEY && (
            <button className="quiet" onClick={() => setPending(DEFAULT_HOTKEY)} title="Back to Ctrl+Shift+D">
              Reset
            </button>
          )}
          {dirty && (
            <button className="primary small" onClick={() => void save()} disabled={saving}>
              Save
            </button>
          )}
          <input
            className="hotkey"
            readOnly
            value={shown}
            onKeyDown={capture}
            title="Click, then press the new keys"
            aria-label="Shortcut. Click here and press the new keys."
          />
        </div>
      </div>
      {error && <p className="warn note">{error}</p>}
      {status && !status.registered && !error && (
        <p className="warn note">{status.error ?? "Windows didn't accept this shortcut."} Pick another one and save it.</p>
      )}

      <SwitchRow
        title="Skip videos"
        checked={settings?.noVideo ?? true}
        disabled={!settings}
        onChange={(on) => void update({ noVideo: on })}
      />
      <SwitchRow
        title="Start with Windows"
        checked={autostart ?? false}
        disabled={autostart === null}
        onChange={(on) => void toggleAutostart(on)}
      />
      {autostartError && <p className="warn note">{autostartError}</p>}

      <LinkSettings />
    </main>
  );
}
