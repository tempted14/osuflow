import { useEffect, useState } from "react";
import type { Settings } from "../lib/types";
import { PathField } from "./PathField";

export function LazerSettings(props: { settings: Settings | null; onSave: (patch: Partial<Settings>) => Promise<boolean> }) {
  const [exe, setExe] = useState(""); const [data, setData] = useState(""); const [command, setCommand] = useState("");
  const [saving, setSaving] = useState(false); const [message, setMessage] = useState<string | null>(null);
  useEffect(() => { if (props.settings) { setExe(props.settings.lazerExePath ?? ""); setData(props.settings.lazerDataDir ?? ""); setCommand(props.settings.lazerLaunchCommand ?? ""); } }, [props.settings]);
  const save = async () => {
    setSaving(true); setMessage(null);
    const ok = await props.onSave({ lazerExePath: exe.trim() || null, lazerDataDir: data.trim() || null, lazerLaunchCommand: command.trim() || null });
    setSaving(false); setMessage(ok ? "Lazer settings saved." : "Couldn't save these settings. Check the command quotes and try again.");
  };
  return <section className="wine-settings lazer-settings" aria-label="Lazer settings">
    <strong>osu!lazer setup</strong>
    <p className="note">Choose your existing lazer app, or leave blank for detection. Linux AppImage and installed Flatpak are supported.</p>
    <PathField label="Lazer application" value={exe} onChange={setExe} placeholder="Choose your osu!lazer application" />
    <details><summary>Library and launch options</summary>
      <PathField label="Lazer data folder" value={data} onChange={setData} directory placeholder="Automatic — choose only if you moved lazer's files" />
      <p className="note">Choose the folder containing <code>client.realm</code> and <code>files</code>. The library index reads beatmap files and leaves the database untouched.</p>
      <label>Custom launch command<input value={command} onChange={(event) => setCommand(event.target.value)} placeholder={'Optional — use "{file}" for the map'} spellCheck={false} autoCapitalize="off" /></label><p className="note">Use a direct command with <code>{"{file}"}</code>. Quote paths with spaces. Clear this to use the application above.</p>
    </details>
    <p className="note">Maps in lazer's recycle bin may stay marked installed until lazer removes their files.</p>
    {message && <p className="note" role="status">{message}</p>}
    <button className="primary small" disabled={!props.settings || saving} onClick={() => void save()}>{saving ? "Saving…" : "Save lazer settings"}</button>
  </section>;
}
