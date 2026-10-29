import { useId, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

/** A native picker is opened only by the user's Browse click. Pasting a path also works. */
export function PathField(props: { label: string; value: string; onChange: (value: string) => void; directory?: boolean; placeholder?: string }) {
  const id = useId();
  const [error, setError] = useState(false);
  const choose = async () => {
    setError(false);
    try {
      const path = await open({ directory: props.directory ?? false, multiple: false, title: `Choose ${props.label}` });
      if (typeof path === "string") props.onChange(path);
    } catch { setError(true); }
  };
  return <div className="path-field">
    <label htmlFor={id}>{props.label}</label>
    <div className="path-picker"><input id={id} value={props.value} onChange={(event) => props.onChange(event.target.value)} placeholder={props.placeholder} autoCapitalize="off" spellCheck={false} /><button className="quiet" type="button" aria-label={`Browse for ${props.label}`} onClick={() => void choose()}>Browse…</button></div>
    {error && <p className="warn note">Couldn't open the file picker. You can paste the path instead.</p>}
  </div>;
}
