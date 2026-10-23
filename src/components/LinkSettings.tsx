import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "../lib/api";
import { browserName, linkMode } from "../lib/links";
import type { LinkHandlerStatus } from "../lib/types";

/**
 * The opt-in "Beatmap links" section. Windows doesn't let an app make itself the default
 * browser, so turning it on is two steps: osu!flow registers itself, then the user chooses it
 * in Default apps. The status refreshes on its own so the second step ticks off when it happens.
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

  if (!status || !mode) return error ? <p className="warn">{error}</p> : null;
  const browser = browserName(status.previousBrowser);

  return (
    <section className="group">
      <h2>Beatmap links</h2>
      <p>
        Click a beatmap link in osu! multiplayer, chat or Discord and osu!flow downloads the map instead of
        opening a web page. Every other link still opens in {browser}.
      </p>

      <ol className="steps">
        <li className={status.enabled && status.registered ? "ok" : ""}>
          Turn it on here
        </li>
        <li className={status.isDefault ? "ok" : ""}>
          Choose osu!flow in Windows' Default apps
          <span className="muted"> (Windows only lets you do this step)</span>
        </li>
      </ol>

      {mode === "off" && (
        <p className="muted">Off. Nothing about your links changes until you turn this on.</p>
      )}
      {mode === "waiting" && (
        <p className="warn">
          Waiting for you to choose osu!flow in Default apps. Until then links keep opening as usual.
        </p>
      )}
      {mode === "active" && <p className="ok-text">On. Beatmap links are downloaded; other links go to {browser}.</p>}
      {mode === "forwarding" && (
        <p className="warn">
          Switched off, but Windows still sends your links to osu!flow, which passes them on to {browser}.
          Choose {browser} in Default apps to stop that.
        </p>
      )}

      <div className="row">
        {!status.enabled ? (
          <button className="primary" onClick={() => void turnOn()} disabled={busy}>
            Turn on
          </button>
        ) : (
          <button onClick={() => void run(() => api.setLinkHandler(false))} disabled={busy}>
            Turn off
          </button>
        )}
        {(status.enabled || status.isDefault) && (
          <button onClick={() => void run(api.openDefaultApps)} disabled={busy}>
            Open Default apps
          </button>
        )}
        {status.registered && !status.enabled && !status.isDefault && (
          <button onClick={() => void run(api.removeLinkRegistration)} disabled={busy}>
            Remove from Windows
          </button>
        )}
      </div>
      {error && <p className="warn">{error}</p>}
    </section>
  );
}
