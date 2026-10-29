import { useEffect, useState } from "react";
import { PathField } from "./PathField";
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
      <p className="note">osu-winello is detected automatically. Leave Wine prefix and command blank so its launcher chooses the right runtime. Use paths on this computer. Leave a field blank to detect it automatically. osu! stable is required.</p>
      <PathField label="osu!.exe" value={exe} onChange={setExe} placeholder="/path/to/osu!/osu!.exe" />
      <PathField label="Songs folder" value={songs} onChange={setSongs} placeholder="/path/to/osu!/Songs" directory />
      <PathField label="Wine prefix" value={prefix} onChange={setPrefix} placeholder="/path/to/wine-prefix" directory />
      <label>Custom launch command<input value={command} onChange={(event) => setCommand(event.target.value)} placeholder="osu-wine --osuhandler {file}" autoCapitalize="off" spellCheck={false} /></label>
      <p className="note">Use <code>{"{file}"}</code> for the downloaded map. Quote paths containing spaces. The command runs directly, without shell expansion.</p>
      {error && <p className="warn note" role="alert">{error}</p>}
      <button className="primary small" disabled={!props.settings || saving} onClick={() => void save()}>{saving ? "Saving…" : "Save install settings"}</button>
    </details>
  );
}
