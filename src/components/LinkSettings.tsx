import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "../lib/api";
import { browserName, linkMode } from "../lib/links";
import type { LinkHandlerStatus } from "../lib/types";

/**
 * The opt-in "Beatmap links" row. Windows doesn't let an app make itself the default browser, so
 * turning it on is two steps: osu!flow registers itself, then the user chooses it in Default apps.
 * The status refreshes on its own so the second step ticks off when it happens.
 */
export function LinkSettings() {
  const [status, setStatus] = useState<LinkHandlerStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setStatus(await api.getLinkHandlerStatus());
    } catch (failure) {
      setError(String(failure));
    }
  }, []);

  useEffect(() => {
    void refresh();
    const unlisten = listen("settings-changed", () => void refresh());
    // Coming back from Windows' Default apps window is when the answer may have changed.
    window.addEventListener("focus", refresh);
    return () => {
      window.removeEventListener("focus", refresh);
      void unlisten.then((stop) => stop());
    };
  }, [refresh]);

  const mode = status ? linkMode(status) : null;
  const waiting = mode === "waiting";
  useEffect(() => {
    if (!waiting) return;
    const timer = setInterval(() => void refresh(), 3000);
    return () => clearInterval(timer);
  }, [waiting, refresh]);

  const run = async (action: () => Promise<LinkHandlerStatus | void>) => {
    setBusy(true);
    setError(null);
    try {
      const next = await action();
      if (next) setStatus(next);
    } catch (failure) {
      setError(String(failure));
    } finally {
      setBusy(false);
    }
  };

  const turnOn = () =>
    run(async () => {
      const next = await api.setLinkHandler(true);
      // Land the user on osu!flow's own entry so the last step is one click.
      if (!next.isDefault) await api.openDefaultApps();
      return next;
    });

  if (!status || !mode) return error ? <p className="warn note">{error}</p> : null;
  const browser = browserName(status.previousBrowser);

  const line = {
    off: `Download beatmap links from chat and lobbies. Other links still open in ${browser}.`,
    waiting: "Last step: choose osu!flow in Windows' Default apps.",
    active: `On. Other links still open in ${browser}.`,
    forwarding: `Off, but Windows still sends links here. Choose ${browser} in Default apps to stop that.`,
  }[mode];
  const tone = mode === "waiting" || mode === "forwarding" ? "warn" : mode === "active" ? "ok-text" : "";

  return (
    <div className="setting wrap">
      <div className="setting-text">
        <strong>Beatmap links</strong>
        <small className={tone}>{line}</small>
      </div>
      <div className="actions">
        {!status.enabled ? (
          <button className="primary small" onClick={() => void turnOn()} disabled={busy}>
            Turn on
          </button>
        ) : (
          <button onClick={() => void run(() => api.setLinkHandler(false))} disabled={busy}>
            Turn off
          </button>
        )}
        {(status.enabled || status.isDefault) && (
          <button onClick={() => void run(api.openDefaultApps)} disabled={busy}>
            Default apps
          </button>
        )}
        {status.registered && !status.enabled && !status.isDefault && (
          <button
            className="quiet"
            onClick={() => void run(api.removeLinkRegistration)}
            disabled={busy}
            title="Remove osu!flow from Windows' list of browsers"
          >
            Remove
          </button>
        )}
      </div>
      {error && <p className="warn note wide">{error}</p>}
    </div>
  );
}
