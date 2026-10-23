// Thin typed wrappers over the Tauri commands in src-tauri/src/commands.rs.
import { invoke } from "@tauri-apps/api/core";
import type {
  Beatmapset,
  ImportMethod,
  OsuInstall,
  SearchQuery,
  SearchResponse,
  Settings,
} from "./types";

export const search = (query: SearchQuery) => invoke<SearchResponse>("search", { query });

/** Resolves once the map has been handed to osu!; progress arrives as `download-*` events. */
export const downloadBeatmapset = (setId: number) =>
  invoke<ImportMethod>("download_beatmapset", { setId });

export const cancelDownload = (setId: number) => invoke<boolean>("cancel_download", { setId });

export const resolveSetId = (beatmapId: number) => invoke<number>("resolve_set_id", { beatmapId });

export const lookupBeatmapset = (setId: number) =>
  invoke<Beatmapset>("lookup_beatmapset", { setId });

export const detectOsu = () => invoke<OsuInstall | null>("detect_osu");

export const getSettings = () => invoke<Settings>("get_settings");

export const setSettings = (settings: Settings) => invoke<Settings>("set_settings", { settings });
