import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "../lib/api";
import type { Settings } from "../lib/types";

/** The saved settings, kept in sync across windows through the `settings-changed` event. */
export function useSettings() {
  const [settings, setSettings] = useState<Settings | null>(null);

  const reload = useCallback(async () => {
    try {
      setSettings(await api.getSettings());
    } catch {
      // Keep whatever is shown; the next change or reload will try again.
    }
  }, []);

  useEffect(() => {
    void reload();
    const unlisten = listen<Settings>("settings-changed", ({ payload }) => setSettings(payload));
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, [reload]);

  const update = useCallback(
    async (patch: Partial<Settings>) => {
      if (!settings) return false;
      const next = { ...settings, ...patch };
      setSettings(next); // optimistic, so toggles feel instant
      try {
        setSettings(await api.setSettings(next));
        return true;
      } catch {
        await reload();
        return false;
      }
    },
    [settings, reload],
  );

  return { settings, update, reload };
}
