import { useEffect, useState } from "react";
import type { Settings } from "../lib/types";

/** Explicit host paths for Wine installs that cannot be found automatically. */
export function WineSettings(props: { settings: Settings | null; onSave: (patch: Partial<Settings>) => Promise<boolean>; onSaved: () => void }) {
  const [exe, setExe] = useState("");
  const [songs, setSongs] = useState("");
  const [prefix, setPrefix] = useState("");
  const [command, setCommand] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!props.settings) return;
    setExe(props.settings.osuExePath ?? ""); setSongs(props.settings.songsDir ?? "");
    setPrefix(props.settings.winePrefix ?? ""); setCommand(props.settings.osuLaunchCommand ?? "");
  }, [props.settings]);
  const save = async () => {
    setSaving(true); setError(null);
    const ok = await props.onSave({ osuExePath: exe.trim() || null, songsDir: songs.trim() || null,
      winePrefix: prefix.trim() || null, osuLaunchCommand: command.trim() || null });
    setSaving(false);
    if (ok) props.onSaved(); else setError("Couldn't save these settings. Check the paths and command quotes, then try again.");
  };
  return (
    <details className="wine-settings">
      <summary>Wine and install settings</summary>
      <p className="note">Use paths on this computer. Leave a field blank to detect it automatically. osu! stable is required.</p>
      <label>osu!.exe<input value={exe} onChange={(event) => setExe(event.target.value)} placeholder="/path/to/osu!/osu!.exe" autoCapitalize="off" spellCheck={false} /></label>
      <label>Songs folder<input value={songs} onChange={(event) => setSongs(event.target.value)} placeholder="/path/to/osu!/Songs" autoCapitalize="off" spellCheck={false} /></label>
      <label>Wine prefix<input value={prefix} onChange={(event) => setPrefix(event.target.value)} placeholder="/path/to/wine-prefix" autoCapitalize="off" spellCheck={false} /></label>
      <label>Custom launch command<input value={command} onChange={(event) => setCommand(event.target.value)} placeholder="osu-wine --osuhandler {file}" autoCapitalize="off" spellCheck={false} /></label>
      <p className="note">Use <code>{"{file}"}</code> for the downloaded map. Quote paths containing spaces. The command runs directly, without shell expansion.</p>
      {error && <p className="warn note" role="alert">{error}</p>}
      <button className="primary small" disabled={!props.settings || saving} onClick={() => void save()}>{saving ? "Saving…" : "Save install settings"}</button>
    </details>
  );
}
