import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "../lib/api";
import { applyLibraryDelta } from "../lib/results";
import type { LibraryDelta } from "../lib/types";

/** Ids of the beatmapsets already in the Songs folder, updated live as maps are imported. */
export function useInstalled() {
  const [installed, setInstalled] = useState<ReadonlySet<number>>(new Set());

  const refresh = useCallback(async () => {
    try {
      setInstalled(new Set(await api.getInstalled()));
    } catch {
      // osu! not found yet: nothing is marked installed.
    }
  }, []);

  useEffect(() => {
    void refresh();
    const unlisten = listen<LibraryDelta>("library-changed", ({ payload }) =>
      setInstalled((current) => applyLibraryDelta(current, payload)),
    );
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, [refresh]);

  return { installed, refresh };
}
