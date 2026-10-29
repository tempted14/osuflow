import { useEffect, useState } from "react";
import type { KeyboardEvent } from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "../lib/api";
import { LinkSettings } from "../components/LinkSettings";
import { LazerSettings } from "../components/LazerSettings";
import { WineSettings } from "../components/WineSettings";
import { displayHotkey } from "../lib/platform";
import type { PlatformInfo } from "../lib/platform";
import { Logo } from "../components/Logo";
import { useSettings } from "../hooks/useSettings";
import { hotkeyFromEvent } from "../lib/hotkey";
import type { HotkeyStatus, OsuInstall, GameClient } from "../lib/types";


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
  const [platform, setPlatform] = useState<PlatformInfo | null>(null);
  const [install, setInstall] = useState<OsuInstall | null | undefined>(undefined);
  const [status, setStatus] = useState<HotkeyStatus | null>(null);
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [pending, setPending] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [autostartError, setAutostartError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [clientError, setClientError] = useState<string | null>(null);
  const lazer = settings?.gameClient === "lazer";

  useEffect(() => {
    api.platform().then(setPlatform).catch(() => undefined);
    api.getHotkeyStatus().then(setStatus).catch(() => undefined);
    api.getAutostart().then(setAutostart).catch(() => setAutostart(false));
    // The tray menu can switch it too.
    const unlisten = listen<boolean>("autostart-changed", ({ payload }) => setAutostart(payload));
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, []);

  useEffect(() => {
    if (!settings) return;
    let current = true; setInstall(undefined);
    api.detectOsu().then((install) => { if (current) setInstall(install); }).catch(() => { if (current) setInstall(null); });
    void api.getInstalled().catch(() => undefined);
    return () => { current = false; };
  }, [settings?.gameClient, settings?.lazerExePath, settings?.lazerDataDir, settings?.lazerLaunchCommand, settings?.osuExePath, settings?.songsDir, settings?.winePrefix]);

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

  const defaultHotkey = platform?.defaultHotkey ?? "Ctrl+Shift+D";
  const os = platform?.os ?? "windows";
  const shown = pending ?? status?.hotkey ?? "";
  const dirty = pending !== null && pending !== status?.hotkey;

  const capture = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Tab") return; // keep keyboard navigation working
    event.preventDefault();
    const chord = hotkeyFromEvent(event.nativeEvent, os);
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

      {platform?.experimental && <p className="note warn">Experimental support for Linux and macOS. This build still needs testing on real systems.</p>}
      {platform?.wayland && <p className="note">Wayland does not support this global shortcut. Use Search maps in the tray, or bind <code>osuflow --toggle</code> in your keyboard settings.</p>}

      <label className="setting client-choice">
        <strong>Game client</strong>
        <select aria-label="Game client" value={settings?.gameClient ?? "stable"} disabled={!settings} onChange={(event) => {
          setClientError(null); void update({ gameClient: event.target.value as GameClient }).then((ok) => { if (!ok) setClientError("Couldn't save the client choice. Try again."); });
        }}>
          <option value="stable">osu! stable</option>
          <option value="lazer">osu!lazer</option>
        </select>
      </label>
      {clientError && <p className="warn note" role="alert">{clientError}</p>}
      {lazer && <p className="note">Lazer imports are experimental and still need native game testing. Search, previews and downloads work the same.</p>}
      <div className="setting">
        <div className="setting-text">
          <strong>{lazer ? "osu!lazer" : "osu!"}</strong>
          {install === undefined && <small>Looking…</small>}
          {install === null && (
            <small className="warn">Not found. {lazer ? "Choose the osu!lazer application below." : platform?.experimental ? "Choose the osu! stable executable or Songs folder below." : "Maps are saved, but can't be imported until osu! is installed."}</small>
          )}
          {install && (
            <small className="path" title={`${install.exe}\n${lazer ? "Lazer library" : "Maps go into"} ${install.songsDir}`}>
              {install.source === "customCommand" ? "Custom launcher" : install.source === "flatpak" ? "Flatpak (sh.ppy.osu)" : install.dir}
            </small>
          )}
          {install?.exclusiveFullscreen && (
            <small className="warn">Exclusive fullscreen hides the overlay. Use borderless or windowed.</small>
          )}
        </div>
        {install && <span className="status-dot" role="img" aria-label="osu! found" />}
      </div>

      {lazer && <LazerSettings settings={settings} onSave={update} />}
      <div className="setting">
        <strong>Shortcut</strong>
        <div className="actions">
          {shown !== defaultHotkey && (
            <button className="quiet" onClick={() => setPending(defaultHotkey)} title={`Back to ${displayHotkey(defaultHotkey, os)}`}>
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
            value={displayHotkey(shown, os)}
            onKeyDown={capture}
            title="Click, then press the new keys"
            aria-label="Shortcut. Click here and press the new keys."
          />
        </div>
      </div>
      {error && <p className="warn note">{error}</p>}
      {status && !status.registered && !error && !platform?.wayland && (
        <p className="warn note">{status.error ?? `${os === "windows" ? "Windows" : "The system"} didn't accept this shortcut.`} Pick another one and save it.</p>
      )}

      <SwitchRow
        title="Skip videos"
        checked={settings?.noVideo ?? true}
        disabled={!settings}
        onChange={(on) => void update({ noVideo: on })}
      />
      <SwitchRow
        title={os === "windows" ? "Start with Windows" : "Start at login"}
        checked={autostart ?? false}
        disabled={autostart === null}
        onChange={(on) => void toggleAutostart(on)}
      />
      {autostartError && <p className="warn note">{autostartError}</p>}

      {platform?.os === "windows" && <LinkSettings />}
      {!lazer && platform?.experimental && <WineSettings settings={settings} onSave={update} onSaved={() => { void api.detectOsu().then(setInstall).catch(() => setInstall(null)); void api.getInstalled().catch(() => undefined); }} />}
      <p className="note"><button className="link" onClick={() => void api.openDownloadsFolder().catch((failure) => setError(String(failure)))}>Open downloads folder</button></p>
    </main>
  );
}
